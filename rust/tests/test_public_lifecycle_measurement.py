"""Controller pipeline/worker transport controls; synthetic timings are not samples."""

from __future__ import annotations

import argparse
import builtins
import copy
import importlib.util
import io
import itertools
import json
import platform
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import types
import unittest
import venv
import zipfile
from pathlib import Path
from unittest import mock

TOOLS = Path(__file__).resolve().parents[1] / "tools"


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, TOOLS / file)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


controller = module("public_measure_controller", "measure_public_lifecycle.py")
worker = module("public_measure_worker", "public_lifecycle_python_worker.py")


def dump(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False) + "\n", encoding="utf-8")


class Pipeline(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="public-lifecycle-controls-")
        self.root = Path(self.tmp.name).resolve()
        self.repo = self.root / "checkout"
        self.repo.mkdir()
        # Independent tiny original reports include exact binary64 bits, a null,
        # non-ASCII text and output projection order distinct from derived order.
        self.row = [
            {"type": "float", "value": "3fb999999999999a"},
            {"type": "str", "value": 'é\\"path'},
            {"type": "missing", "value": None},
        ]
        for case, filename in controller.OUTPUTS.items():
            base = self.repo / "benchmarks" / case
            (base / "input").mkdir(parents=True)
            (base / "expected").mkdir()
            (base / "spec.yaml").write_text("independent original fixture\n")
            (base / "input" / "source.csv").write_bytes(b"VALUE\n0.1\n")
            csv = b"TEXT,NUMBER,EMPTY\noriginal,0.1,\n"
            (base / "expected" / filename).write_bytes(csv)
            report = {
                "report_version": "1",
                "runtime": "python",
                "backend": "rust",
                "runtime_version": "fixture-runtime",
                "engine_version": "fixture-engine",
                "example": case,
                "outcome": "success",
                "diagnostics": [],
                "error": None,
                "unsupported": [],
                "handler_counts": {"ordered_sum": 1},
                "source_reads": {"created": 1},
                "callbacks": [],
                "verifications": [],
                "nodes": [{"specification": "spec.yaml", "name": "VALUE"}],
                "tables": [
                    {
                        "specification": "spec.yaml",
                        "name": "output",
                        "stage": "derived",
                        "columns": ["NUMBER", "TEXT", "EMPTY"],
                        "types": ["float", "str", "int"],
                        "rows": [self.row],
                    }
                ],
                "artifacts": [
                    {
                        "profile": "csv",
                        "name": filename[:-4],
                        "columns": ["TEXT", "NUMBER", "EMPTY"],
                        "types": ["str", "float", "int"],
                        "row_count": 1,
                        "content": csv.decode(),
                        "byte_length": len(csv),
                    }
                ],
            }
            dump(
                self.repo
                / f"rust/crates/yamaa-adapters/tests/fixtures/specs/{case}.json",
                report,
            )
        (self.repo / "yaml").mkdir()
        (self.repo / "yaml" / "schema_domain.yaml").write_text("original schema\n")
        (self.repo / "rust" / "Cargo.lock").write_bytes(
            b"independent native source closure\n"
        )
        subprocess.run(["git", "init", "-q", str(self.repo)], check=True)
        subprocess.run(["git", "add", "."], cwd=self.repo, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Harness",
                "-c",
                "user.email=harness@example.invalid",
                "commit",
                "-qm",
                "independent originals",
            ],
            cwd=self.repo,
            check=True,
        )
        revision = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=self.repo, text=True
        ).strip()
        self.installed = {}
        for host in ("python", "r"):
            package = self.root / (host + " library é") / "yamaa"
            package.mkdir(parents=True)
            native = package / "native.bin"
            native.write_bytes(b"native original installed bytes")
            artifact = self.root / (host + ".artifact")
            with tarfile.open(artifact, "w:gz") as archive:
                data = (self.repo / "rust" / "Cargo.lock").read_bytes()
                member = tarfile.TarInfo("test-source/rust/Cargo.lock")
                member.size = len(data)
                archive.addfile(member, io.BytesIO(data))
            record = {
                "source_revision": revision,
                "artifact_reference": "test:" + host,
                "artifact_path": str(artifact),
                "package_form": "source",
                "host": host,
                "runtime_version": "test-runtime",
                "package_version": "test-package",
                "engine_version": "test-engine",
                "toolchain": None,
                "installed_origins": {"package": str(package), "native": str(native)},
                "system": platform.system(),
                "cpu": None,
            }
            attestation = self.root / (host + "-installation.json")
            dump(
                attestation,
                {
                    "evidence": {
                        k: record[k]
                        for k in (
                            "source_revision",
                            "artifact_reference",
                            "package_form",
                        )
                    },
                    "installation": {
                        "runtime_version": "test-runtime",
                        "host_package_version": "test-package",
                        "core_version": "test-engine",
                        "package_location": str(
                            native if host == "python" else package
                        ),
                    },
                },
            )
            record["installation_evidence"] = str(attestation)
            if host == "r":
                collectors = {}
                for name, version in (
                    ("jsonlite", "2.0.0"),
                    ("microbenchmark", "1.5.0"),
                ):
                    origin = self.root / "collectors" / name
                    origin.mkdir(parents=True)
                    collectors[name] = {"origin": str(origin), "version": version}
                timer = Path(collectors["microbenchmark"]["origin"]) / "timer.bin"
                timer.write_bytes(b"built installed clock")
                config = self.root / "clock-config.h"
                if platform.system() == "Darwin":
                    config.write_text("#define MB_HAVE_MACH_TIME 1\n")
                    provider = "mach_absolute_time"
                elif platform.system() == "Windows":
                    config.write_text("WIN32 compilation attestation\n")
                    provider = "QueryPerformanceCounter"
                else:
                    config.write_text(
                        "#define MB_HAVE_CLOCK_GETTIME 1\n#define MB_CLOCKID_T CLOCK_MONOTONIC_RAW\n"
                    )
                    provider = "clock_gettime/CLOCK_MONOTONIC_RAW"
                record.update(
                    collectors=collectors,
                    clock_evidence={
                        "provider": provider,
                        "build_config": str(config),
                        "built_library": str(timer),
                        "installed_library": str(timer),
                    },
                )
            path = self.root / (host + "-provenance.json")
            dump(path, record)
            self.installed[host] = record
        self.args = argparse.Namespace(
            repository=self.repo,
            revision=revision,
            output=self.root / "evidence",
            python_provenance=self.root / "python-provenance.json",
            r_provenance=self.root / "r-provenance.json",
            python_executable=Path(sys.executable),
            r_executable=Path(sys.executable),
            purpose="correctness-smoke",
            repeats=1,
            warmups=1,
            compilation_clear=True,
        )
        self.calls = []

    def tearDown(self):
        self.tmp.cleanup()

    def fake_launch(self, command, child, mutate=None):
        """Simulated installed process transport; all clock values are authored."""
        self.calls.append((command, child))
        request = json.loads((child / "request.json").read_text())
        host = request["host"]
        declared = request["provenance"]
        original = json.loads(
            (
                self.repo
                / f"rust/crates/yamaa-adapters/tests/fixtures/specs/{request['case']}.json"
            ).read_text()
        )
        timing = {"start_ns": 100, "end_ns": 140, "duration_ns": 40}
        if host == "r":
            # Independently authored binary64 representations of 100, 140, 40.
            timing = {
                "representation": "binary64-nanoseconds",
                "start_ns": {"decimal": "100", "binary64_bits": "4059000000000000"},
                "end_ns": {"decimal": "140", "binary64_bits": "4061800000000000"},
                "duration_ns": {"decimal": "40", "binary64_bits": "4044000000000000"},
            }
        response = {
            "protocol": "public-lifecycle/1",
            "child": request["child"],
            "import_duration": timing,
            "failure": None,
            "repetitions": [],
        }
        for rep in request["repetitions"]:
            index = rep["index"]
            prefix = f"rep-{index}"
            observed = {
                k: declared[k]
                for k in (
                    "host",
                    "runtime_version",
                    "package_version",
                    "engine_version",
                    "system",
                    "installed_origins",
                )
            }
            observed["report_runtime_version"] = (
                "test-runtime (independently observed host control)"
                if host == "python"
                else "test-runtime"
            )
            observed["clock"] = {"monotonic": True, "adjustable": False}
            if host == "r":
                observed.update(
                    collectors=declared["collectors"],
                    clock_library=declared["clock_evidence"]["installed_library"],
                )
                observed["clock"] = {
                    "provider": declared["clock_evidence"]["provider"],
                    "api": "microbenchmark::get_nanotime",
                }
            row = {
                "index": index,
                "declaration": rep,
                "status": "success",
                "metadata": observed,
                "native_call_order": ["public_build", "metadata_after_build"],
                "unavailable": copy.deepcopy(controller.UNAVAILABLE),
                "durations": {name: timing for name in controller.BOUNDARIES[1:]},
                "reports": {},
                "views": prefix + "-views.json",
                "save_returns": [True, True],
                "saved_files": {},
                "rss": None,
                "failure": None,
                "evidence_capture_failures": [],
            }
            if host == "python" and platform.system() in {"Darwin", "Linux"}:
                raw, scale, unit = (
                    (4096, 1, "bytes")
                    if platform.system() == "Darwin"
                    else (4096, 1024, "KiB")
                )
                row["rss"] = {
                    "provider": "resource.getrusage(RUSAGE_SELF).ru_maxrss",
                    "raw": raw,
                    "original_unit": unit,
                    "bytes": raw * scale,
                    "scope": "whole-child-lifetime; cumulative across warm repetitions",
                }
            # Independently written oracle transport (no controller expected_* call).
            frame = {
                "columns": [["TEXT", "str"], ["NUMBER", "float"], ["EMPTY", "int"]],
                "row_count": 1,
                "rows": [[self.row[1], self.row[0], self.row[2]]],
                "host_schema": ["actual host declarations"],
            }
            views = {
                "output": frame,
                "output_after_save_2": copy.deepcopy(frame),
                "issues": {
                    "columns": [
                        ["phase", "str"],
                        ["condition", "str"],
                        ["requirement", "str"],
                        ["spec_paths", "list[str]"],
                        ["context", "str"],
                    ],
                    "row_count": 0,
                    "rows": [],
                },
                "verification_log": None,
                "warning_log": None,
            }
            dump(child / row["views"], views)
            for phase in ("build", "save_1", "save_2"):
                report = copy.deepcopy(original)
                report.update(
                    runtime=host,
                    runtime_version=observed["report_runtime_version"],
                    engine_version="test-engine",
                    example="domain",
                )
                for item in report["nodes"] + report["tables"]:
                    item["specification"] = rep["entry"]
                if phase == "build":
                    report["artifacts"] = []
                filename = prefix + "-" + phase + "-report.json"
                dump(child / filename, report)
                row["reports"][phase] = filename
                if phase != "build":
                    filename = prefix + "-" + phase + ".csv"
                    (child / filename).write_bytes(
                        original["artifacts"][0]["content"].encode()
                    )
                    (Path(rep["work_directory"]) / rep["output_name"]).write_bytes(
                        original["artifacts"][0]["content"].encode()
                    )
                    row["saved_files"][phase] = filename
            response["repetitions"].append(row)
        if mutate:
            mutate(request, response, child)
        dump(child / "response.json", response)
        (child / "stdout.txt").write_bytes(b"every raw worker stdout\n")
        (child / "stderr.txt").write_bytes(b"every raw worker stderr\n")
        return {"exit_code": 0, "pid": 123}

    def collect(self, mutate=None, probe=None):
        return controller.collect(
            self.args,
            launcher=lambda command, child: self.fake_launch(command, child, mutate),
            probe=probe or (lambda: {"clear": True, "compilers": []}),
        )

    def test_complete_pipeline_retains_every_child_warmup_and_original(self):
        result = self.collect()
        self.assertTrue(result["accepted"])
        self.assertEqual(len(self.calls), 12)
        self.assertNotIn(
            "distribution", result
        )  # correctness-only, no characterization
        reps = []
        for row in result["children"]:
            response = json.loads(
                (self.args.output / row["child"] / "response.json").read_text()
            )
            reps.extend(
                r["declaration"]["work_directory"] for r in response["repetitions"]
            )
            self.assertEqual(
                len(response["repetitions"]), 1 if row["mode"] == "cold" else 2
            )
            if row["mode"] == "warm":
                self.assertEqual(
                    [r["warmup"] for r in row["acceptance"]], [True, False]
                )
            for name in ("stdout.txt", "stderr.txt", "compilation-probe.json"):
                self.assertTrue((self.args.output / row["child"] / name).exists())
        self.assertEqual(len(reps), len(set(reps)))
        for row in result["original_byte_comparisons"]:
            self.assertEqual(
                (self.args.output / "originals" / row["path"]).read_bytes(),
                controller.git_bytes(self.repo, self.args.revision, row["path"]),
            )

    def test_distinct_harness_and_truth_repositories_keep_untracked_bytes_authoritative(
        self,
    ):
        harness_repo = self.root / "different-harness-checkout"
        harness_repo.mkdir()
        (harness_repo / "README").write_text("separate harness baseline\n")
        subprocess.run(["git", "init", "-q", str(harness_repo)], check=True)
        subprocess.run(["git", "add", "README"], cwd=harness_repo, check=True)
        subprocess.run(
            [
                "git",
                "-c",
                "user.name=Harness",
                "-c",
                "user.email=harness@example.invalid",
                "commit",
                "-qm",
                "different baseline",
            ],
            cwd=harness_repo,
            check=True,
        )
        harness_head = subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=harness_repo, text=True
        ).strip()
        for name in (
            "measure_public_lifecycle.py",
            "public_lifecycle_worker_protocol.json",
            *controller.WORKERS.values(),
        ):
            shutil.copyfile(TOOLS / name, harness_repo / name)  # deliberately untracked
        with mock.patch.object(controller, "HERE", harness_repo):
            _, output, plan = controller.prepare(self.args)  # no worker/timer calls
        self.assertEqual(plan["truth_repository_head"], self.args.revision)
        self.assertEqual(plan["harness_source"]["checkout_head"], harness_head)
        self.assertNotEqual(harness_head, self.args.revision)
        self.assertIn(
            "does not prove untracked", plan["harness_source"]["identity_limit"]
        )
        self.assertEqual(
            (output / "harness" / "measure_public_lifecycle.py").read_bytes(),
            (harness_repo / "measure_public_lifecycle.py").read_bytes(),
        )
        outside = self.root / "standalone-harness"
        outside.mkdir()
        self.assertIsNone(controller.surrounding_git_head(outside))

    def test_truth_corruptions_reject_complete_batch_without_discarding_raw_results(
        self,
    ):
        def changed_file(key, change):
            def mutate(request, response, child):
                path = child / response["repetitions"][0][key]
                data = json.loads(path.read_text())
                change(data)
                dump(path, data)

            return mutate

        def corrupt_report(request, response, child):
            path = child / response["repetitions"][0]["reports"]["build"]
            data = json.loads(path.read_text())
            data["source_reads"]["created"] = 0
            dump(path, data)

        def corrupt_saved(request, response, child):
            (child / response["repetitions"][0]["saved_files"]["save_2"]).write_bytes(
                b"different held bytes"
            )

        controls = {
            "float-bit": changed_file(
                "views",
                lambda x: x["output"]["rows"][0][1].update(value="3fb999999999999b"),
            ),
            "type": changed_file(
                "views", lambda x: x["output"]["columns"][1].__setitem__(1, "int")
            ),
            "order": changed_file("views", lambda x: x["output"]["columns"].reverse()),
            "issues": changed_file(
                "views", lambda x: x["issues"].update(rows=[["invented"]])
            ),
            "logs": changed_file(
                "views", lambda x: x.update(verification_log={"unexpected": True})
            ),
            "report": corrupt_report,
            "saved": corrupt_saved,
        }
        for name, mutation in controls.items():
            with self.subTest(name=name):
                self.args.output = self.root / ("evidence-" + name)
                result = self.collect(mutation)
                self.assertFalse(result["accepted"])
                self.assertEqual(len(result["children"]), 12)
                self.assertTrue(
                    (
                        self.args.output
                        / result["children"][0]["child"]
                        / "response.json"
                    ).exists()
                )
                self.assertTrue(
                    any(
                        not a["accepted"]
                        for row in result["children"]
                        for a in row["acceptance"]
                    )
                )

    def test_actual_origin_version_and_call_order_controls(self):
        def origin(request, response, child):
            response["repetitions"][0]["metadata"]["installed_origins"]["package"] = (
                str(self.repo)
            )

        def version(request, response, child):
            response["repetitions"][0]["metadata"]["engine_version"] = (
                "different-engine"
            )

        def premature(request, response, child):
            response["repetitions"][0]["native_call_order"].reverse()

        def missing_metric(request, response, child):
            response["repetitions"][0]["durations"]["public_build"] = None

        def invented_callback_count(request, response, child):
            response["repetitions"][0]["unavailable"]["actual_callback_counts"] = 0

        def secondary_transport(request, response, child):
            response["repetitions"][0]["evidence_capture_failures"] = [
                {"message": "secondary cause"}
            ]

        for name, mutation in (
            ("origin", origin),
            ("version", version),
            ("call-order", premature),
            ("timer", missing_metric),
            ("count", invented_callback_count),
            ("secondary-transport", secondary_transport),
        ):
            with self.subTest(name=name):
                self.args.output = self.root / ("identity-" + name)
                self.assertFalse(self.collect(mutation)["accepted"])

    def test_dirty_original_schema_and_fixture_refuse_before_workers(self):
        for path in (
            "yaml/schema_domain.yaml",
            "benchmarks/schema-lookup/input/source.csv",
        ):
            with self.subTest(path=path):
                original = (self.repo / path).read_bytes()
                (self.repo / path).write_bytes(b"corrupted original")
                self.args.output = self.root / ("dirty-" + Path(path).name)
                with self.assertRaisesRegex(ValueError, "fixture checkout differs"):
                    self.collect()
                self.assertEqual(self.calls, [])
                self.assertEqual(
                    json.loads((self.args.output / "collection.json").read_text())[
                        "state"
                    ],
                    "rejected",
                )
                (self.repo / path).write_bytes(original)

    def test_source_artifact_attestation_and_symlink_origin_controls(self):
        original = copy.deepcopy(self.installed["python"])
        for field, value in (
            ("source_revision", "wrong-source"),
            ("artifact_reference", "wrong-artifact"),
            ("runtime_version", "wrong-runtime"),
        ):
            with self.subTest(field=field):
                record = copy.deepcopy(original)
                record[field] = value
                dump(self.args.python_provenance, record)
                self.args.output = self.root / ("attestation-" + field)
                with self.assertRaisesRegex(ValueError, "evidence differs"):
                    self.collect()
        link = self.root / "looks-installed"
        try:
            link.symlink_to(self.repo, target_is_directory=True)
        except OSError:
            return  # provider unavailable in this control; not a collected sample
        record = copy.deepcopy(original)
        record["installed_origins"]["package"] = str(link)
        dump(self.args.python_provenance, record)
        self.args.output = self.root / "symlink-origin"
        with self.assertRaisesRegex(ValueError, "inside a checkout"):
            self.collect()

    def test_compilation_refusal_retains_all_declared_unlaunched_steps(self):
        result = self.collect(
            probe=lambda: {"clear": False, "compilers": [{"pid": 10, "name": "rustc"}]}
        )
        self.assertFalse(result["accepted"])
        self.assertEqual(len(result["children"]), 12)
        self.assertEqual(self.calls, [])
        self.assertTrue(
            all(
                "compilation" in row["failure"]["message"] for row in result["children"]
            )
        )

    def test_archive_byte_corruption_and_wrong_wheel_binary_refuse_before_launch(self):
        record = self.installed["python"]
        with tarfile.open(record["artifact_path"], "w:gz") as archive:
            data = b"different native source bytes"
            member = tarfile.TarInfo("test-source/rust/Cargo.lock")
            member.size = len(data)
            archive.addfile(member, io.BytesIO(data))
        with self.assertRaisesRegex(ValueError, "pinned Git bytes"):
            self.collect()
        self.assertEqual(self.calls, [])
        wheel = self.root / "incorrect.whl"
        with zipfile.ZipFile(wheel, "w") as archive:
            archive.writestr("yamaa/_native.abi3.so", b"different installed binary")
        record = copy.deepcopy(record)
        record.update(package_form="wheel", artifact_path=str(wheel))
        with self.assertRaisesRegex(ValueError, "declared wheel bytes"):
            controller.artifact_bytes(record, self.repo)

    def test_unavailable_process_inventory_rejects_without_assuming_clear(self):
        def missing_probe():
            raise OSError("process provider unavailable")

        result = self.collect(probe=missing_probe)
        self.assertFalse(result["accepted"])
        self.assertEqual(self.calls, [])
        self.assertEqual(len(result["children"]), 12)

    def test_startup_partial_warm_and_failed_warmup_cannot_be_skipped(self):
        def missing(request, response, child):
            if request["mode"] == "warm":
                response["repetitions"].pop()

        self.assertFalse(self.collect(missing)["accepted"])
        self.args.output = self.root / "failure-warmup"

        def failed(request, response, child):
            if request["mode"] == "warm":
                response["repetitions"][0].update(
                    status="failure", failure={"message": "original cause"}
                )

        result = self.collect(failed)
        self.assertFalse(result["accepted"])
        self.assertTrue(
            all(
                row["acceptance"][1]["accepted"]
                for row in result["children"]
                if row["mode"] == "warm"
            )
        )
        self.args.output = self.root / "startup-failure"

        def launch_failure(command, child):
            (child / "stderr.txt").write_text("original startup failure")
            raise OSError("cannot start child")

        result = controller.collect(
            self.args,
            launcher=launch_failure,
            probe=lambda: {"clear": True, "compilers": []},
        )
        self.assertFalse(result["accepted"])
        self.assertEqual(len(result["children"]), 12)
        self.assertEqual(
            (
                self.args.output / result["children"][0]["child"] / "stderr.txt"
            ).read_text(),
            "original startup failure",
        )

    def test_retained_fixture_corruption_after_launch_and_existing_output_are_refused(
        self,
    ):
        def corrupt(request, response, child):
            (
                self.args.output / "originals" / "yaml" / "schema_domain.yaml"
            ).write_bytes(b"after-stage corruption")

        with self.assertRaisesRegex(ValueError, "retained original bytes changed"):
            self.collect(corrupt)
        self.assertEqual(len(self.calls), 1)
        before = (self.args.output / "collection.json").read_bytes()
        with self.assertRaisesRegex(ValueError, "fresh evidence directory"):
            self.collect()
        self.assertEqual((self.args.output / "collection.json").read_bytes(), before)

    def test_r_clock_fallback_and_library_mismatch_are_refused(self):
        record = copy.deepcopy(self.installed["r"])
        Path(record["clock_evidence"]["build_config"]).write_text(
            "#define MB_HAVE_GETTIMEOFDAY 1\n"
        )
        with self.assertRaisesRegex(ValueError, "monotonic|Windows"):
            self.collect()
        self.assertEqual(self.calls, [])


class Transports(unittest.TestCase):
    def test_real_synthetic_venv_isolated_subprocess_preserves_invocation_context(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            repository = root / "checkout"
            subprocess.run(["git", "init", "-q", str(repository)], check=True)
            environment = root / "synthetic venv é"
            venv.EnvBuilder(with_pip=False, symlinks=(sys.platform != "win32")).create(
                environment
            )
            executable = environment / (
                "Scripts/python.exe" if sys.platform == "win32" else "bin/python"
            )
            selected = controller.worker_executable(executable, repository)
            self.assertEqual(selected, executable)
            purelib = Path(
                subprocess.check_output(
                    [
                        str(selected),
                        "-I",
                        "-c",
                        "import sysconfig; print(sysconfig.get_path('purelib'))",
                    ],
                    text=True,
                ).strip()
            )
            marker = purelib / "public_lifecycle_synthetic_venv_marker.py"
            marker.write_text("identity = 'independently installed synthetic marker'\n")
            script = root / "venv-control.py"
            script.write_text(
                "import json, sys, public_lifecycle_synthetic_venv_marker as marker\nprint(json.dumps({'prefix': sys.prefix, 'marker': marker.identity, 'origin': marker.__file__}))\n"
            )
            command = controller.worker_command(
                "python", selected, script, root / "request.json"
            )
            result = subprocess.run(command, cwd=root, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            observed = json.loads(result.stdout)
            self.assertEqual(Path(observed["prefix"]).resolve(), environment)
            self.assertEqual(Path(observed["origin"]).resolve(), marker)
            self.assertEqual(
                observed["marker"], "independently installed synthetic marker"
            )
            if sys.platform != "win32":
                self.assertTrue(executable.is_symlink())
                wrong = subprocess.run(
                    controller.worker_command(
                        "python", executable.resolve(), script, root / "request.json"
                    ),
                    cwd=root,
                    capture_output=True,
                    text=True,
                )
                self.assertNotEqual(wrong.returncode, 0)
                self.assertIn("public_lifecycle_synthetic_venv_marker", wrong.stderr)
            with self.assertRaisesRegex(ValueError, "inside a checkout"):
                controller.worker_executable(repository / "python", repository)
            with self.assertRaisesRegex(ValueError, "wrong file kind"):
                controller.worker_executable(root, repository)

    def test_actual_jsonlite_large_binary64_roundtrip_without_candidate_or_clock(self):
        executable = shutil.which("Rscript")
        if executable is None:
            self.skipTest("base R unavailable for authored clock transport control")
        pairs = [
            (2540977198640091.0, 2540977224087848.0),
            (9007199254740992.0, 9007199254740994.0),
            (4503599627370495.5, 4503599627370496.0),
            (-0.0, 0.0),
        ]
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root / "clock-transport-control.R"
            script.write_text(
                """
args <- commandArgs(TRUE)
if (!requireNamespace("jsonlite", quietly=TRUE)) quit(status=77L)
stopifnot(!("yamaa" %in% loadedNamespaces()))
expressions <- parse(args[1])
e <- new.env(parent=globalenv())
wanted <- c("write_json", "binary64_number", "timed")
for (x in expressions) if (is.call(x) && identical(x[[1]], as.name("<-")) &&
                          is.symbol(x[[2]]) && as.character(x[[2]]) %in% wanted) eval(x,envir=e)
pairs <- list(c(2540977198640091,2540977224087848),
              c(9007199254740992,9007199254740994),
              c(4503599627370495 + 0.5,4503599627370496),c(-0.0,0.0))
records <- list()
for (pair in pairs) {
  index <- 0L
  e$clock <- function() { index <<- index+1L; pair[index] }
  state <- new.env(parent=emptyenv()); state$durations <- list()
  stopifnot(identical(e$timed(state,"authored",TRUE),TRUE),identical(index,2L))
  records <- c(records,list(state$durations$authored))
}
e$write_json(file.path(args[2],"lossless.json"),records)
pair <- pairs[[1]]
e$write_json(file.path(args[2],"rounded-json.json"),
             list(start_ns=pair[1],end_ns=pair[2],duration_ns=pair[2]-pair[1]))
cat("actual jsonlite authored binary64 transport PASS\\n")
""",
                encoding="utf-8",
            )
            process = subprocess.run(
                [
                    executable,
                    "--vanilla",
                    str(script),
                    str(TOOLS / "public_lifecycle_r_worker.R"),
                    str(root),
                ],
                capture_output=True,
                text=True,
            )
            if process.returncode == 77:
                self.skipTest(
                    "explicit jsonlite collector unavailable for transport control"
                )
            self.assertEqual(process.returncode, 0, process.stderr)
            records = json.loads((root / "lossless.json").read_text())
            self.assertEqual(len(records), len(pairs))
            for record, (start, end) in zip(records, pairs):
                for field, expected in (
                    ("start_ns", start),
                    ("end_ns", end),
                    ("duration_ns", end - start),
                ):
                    self.assertEqual(
                        record[field]["binary64_bits"],
                        struct.pack(">d", expected).hex(),
                    )
                    self.assertEqual(
                        struct.pack(">d", float(record[field]["decimal"])),
                        struct.pack(">d", expected),
                    )
                self.assertEqual(controller.validate_duration(record, "r"), end - start)
            rounded = json.loads((root / "rounded-json.json").read_text())
            self.assertNotEqual(
                rounded["end_ns"] - rounded["start_ns"], rounded["duration_ns"]
            )
            with self.assertRaisesRegex(ValueError, "moved backwards or was adjusted"):
                controller.validate_duration(rounded)
            with self.assertRaisesRegex(ValueError, "requires lossless"):
                controller.validate_duration(rounded, "r")
            decimal_corruption = copy.deepcopy(records[0])
            decimal_corruption["start_ns"]["decimal"] = "1"
            with self.assertRaisesRegex(ValueError, "bytes disagree"):
                controller.validate_duration(decimal_corruption, "r")
            elapsed_corruption = copy.deepcopy(records[0])
            elapsed_corruption["duration_ns"] = {
                "decimal": "1",
                "binary64_bits": "3ff0000000000000",
            }
            with self.assertRaisesRegex(ValueError, "moved backwards or was adjusted"):
                controller.validate_duration(elapsed_corruption, "r")
            with self.assertRaisesRegex(ValueError, "raw integer"):
                controller.validate_duration(records[0], "python")

    def test_real_python_worker_control_uses_new_domains_and_build_before_metadata(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            calls, domains = [], []

            class FakeFrame:
                columns = ["VALUE"]
                dtypes = ["float"]
                schema = {"VALUE": "float"}
                height = 1

                def rows(self):
                    return [(0.1,)]

            class FakeIssues:
                schema = {
                    "phase": "str",
                    "condition": "str",
                    "requirement": "str",
                    "spec_paths": ("list", "str"),
                    "context": "str",
                }
                height = 0

                def rows(self):
                    return []

            class FakeDomain:
                output = FakeFrame()
                issues = FakeIssues()
                verification_log = warning_log = None

                def __init__(self, entry):
                    self.target = Path(entry).parent / "output.csv"
                    self.saves = 0
                    self._native = types.SimpleNamespace(
                        observations=lambda: json.dumps({"saves": self.saves})
                    )

                def save(self):
                    self.saves += 1
                    self.target.write_bytes(b"VALUE\n0.1\n")
                    return True

            def domain(entry):
                calls.append("build")
                result = FakeDomain(entry)
                domains.append(result)
                return result

            def info():
                self.assertEqual(calls[-1], "build")
                calls.append("metadata")
                return {"core_version": "test"}

            candidate = types.ModuleType("yamaa")
            candidate.__file__ = str(root / "installed" / "__init__.py")
            candidate.__version__ = "test"
            candidate.domain = domain
            candidate._native = types.SimpleNamespace(
                __file__=str(root / "installed" / "native.so"), engine_info=info
            )
            polars = types.ModuleType("polars")
            polars.String, polars.Int64, polars.Float64, polars.Date = (
                "str",
                "int",
                "float",
                "date",
            )
            polars.Datetime = type("Datetime", (), {})
            polars.List = lambda x: ("list", x)
            declarations = []
            for i in range(2):
                work = root / f"work-{i}"
                work.mkdir()
                (work / "spec.yaml").write_text("independent worker fixture")
                declarations.append(
                    {
                        "index": i,
                        "warmup": i == 0,
                        "work_directory": str(work),
                        "entry": str(work / "spec.yaml"),
                        "output_name": "output.csv",
                    }
                )
            request = {
                "output": str(root),
                "protocol": "public-lifecycle/1",
                "child": "control",
                "repetitions": declarations,
                "unavailable": controller.UNAVAILABLE,
            }
            # Authored clock sequence: this worker control collects no timing of
            # a real installed candidate while another task may be compiling.
            real_import = builtins.__import__

            def control_import(name, *args, **kwargs):
                if name == "yamaa":
                    return candidate
                if name == "polars":
                    return polars
                return real_import(name, *args, **kwargs)

            with (
                mock.patch("builtins.__import__", side_effect=control_import),
                mock.patch.object(
                    worker.time, "perf_counter_ns", side_effect=itertools.count(100, 10)
                ),
            ):
                self.assertEqual(worker.run(request), 0)
            self.assertEqual(calls, ["build", "metadata", "build", "metadata"])
            self.assertIsNot(domains[0], domains[1])
            response = json.loads((root / "response.json").read_text())
            self.assertEqual(
                [r["declaration"]["warmup"] for r in response["repetitions"]],
                [True, False],
            )
            for row in response["repetitions"]:
                self.assertEqual(
                    [
                        json.loads((root / row["reports"][key]).read_text())["saves"]
                        for key in ("build", "save_1", "save_2")
                    ],
                    [0, 1, 2],
                )
                views = json.loads((root / row["views"]).read_text())
                self.assertEqual(
                    views["output"]["rows"],
                    [[{"type": "float", "value": "3fb999999999999a"}]],
                )

    def test_python_float_bits_dates_int_extremes_and_missing_are_independent(self):
        import datetime

        self.assertEqual(
            worker.cell(0.1, "float"), {"type": "float", "value": "3fb999999999999a"}
        )
        self.assertEqual(
            worker.cell(-0.0, "float"), {"type": "float", "value": "8000000000000000"}
        )
        self.assertEqual(
            worker.cell(-(2**63), "int"),
            {"type": "int", "value": "-9223372036854775808"},
        )
        self.assertEqual(worker.cell(None, "float"), {"type": "missing", "value": None})
        self.assertEqual(
            worker.cell(datetime.date(2025, 1, 2), "date"),
            {"type": "date", "value": "2025-01-02"},
        )

    def test_r_argv_and_json_preserve_windows_quotes_unicode_and_backslashes(self):
        path = 'C:\\users\\é "quoted"\\worker.R'
        command = controller.worker_command(
            "r", "Rscript.exe", path, 'C:\\request "é".json'
        )
        self.assertEqual(
            command, ["Rscript.exe", "--vanilla", path, 'C:\\request "é".json']
        )
        self.assertNotIn("-e", command)
        original = {"entry": 'C:\\users\\é "quoted"\\spec.yaml'}
        self.assertEqual(json.loads(json.dumps(original, ensure_ascii=False)), original)

    def test_rss_null_and_units_and_raw_unadjusted_timer_controls(self):
        controller.validate_rss(None, "r", "Linux")
        controller.validate_rss(None, "python", "Windows")
        with self.assertRaisesRegex(ValueError, "remain null"):
            controller.validate_rss({"bytes": 0}, "r", "Darwin")
        with self.assertRaisesRegex(ValueError, "missing actual"):
            controller.validate_rss(None, "python", "Linux")
        controller.validate_duration(
            {"start_ns": 100, "end_ns": 140, "duration_ns": 40}
        )
        for duration in (
            None,
            {"start_ns": 100, "end_ns": 90, "duration_ns": 0},
            {"start_ns": 100, "end_ns": 140, "duration_ns": 39},
        ):
            with self.assertRaises(ValueError):
                controller.validate_duration(duration)

    def test_actual_r_base_transport_functions_without_candidate_or_timing(self):
        executable = shutil.which("Rscript")
        if executable is None:
            self.skipTest("base R is unavailable for non-performance transport control")
        with tempfile.TemporaryDirectory() as directory:
            script = Path(directory) / "transport-control.R"
            # Select only pure transport functions from the parsed script: no
            # imports, candidate build or timing sample is executed by this test.
            script.write_text(
                """
expressions <- parse(commandArgs(TRUE)[1])
wanted <- c("storage_kind", "cell", "frame", "issues")
for (x in expressions) if (is.call(x) && identical(x[[1]], as.name("<-")) && is.symbol(x[[2]]) &&
                          as.character(x[[2]]) %in% wanted) eval(x)
stopifnot(identical(cell(c(0.1), 1L, "float")$value, "3fb999999999999a"))
stopifnot(identical(cell(c(-0.0), 1L, "float")$value, "8000000000000000"))
stopifnot(identical(cell(as.Date("2025-01-02"), 1L, "date")$value, "2025-01-02"))
x <- structure(c("-9223372036854775808", NA_character_), class="yamaa_int64_vector")
stopifnot(identical(cell(x, 1L, "int")$value, "-9223372036854775808"),
          identical(cell(x, 2L, "int")$type, "missing"))
stopifnot(identical(cell(c(NaN), 1L, "float")$type, "float"))
i <- data.frame(phase="compile", condition="original", requirement="REQ-1", context="{}")
i$spec_paths <- I(list(c("first", "second")))
i <- i[c("phase", "condition", "requirement", "spec_paths", "context")]
copied <- issues(i)
stopifnot(is.null(names(copied$rows[[1]])),
          identical(copied$rows[[1]][[4]], list("first", "second")))
cat("base R independent transport controls PASS\\n")
""",
                encoding="utf-8",
            )
            result = subprocess.run(
                [
                    executable,
                    "--vanilla",
                    str(script),
                    str(TOOLS / "public_lifecycle_r_worker.R"),
                ],
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("controls PASS", result.stdout)


if __name__ == "__main__":
    unittest.main()
