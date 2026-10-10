"""Installed metadata, native project results and original failure retention."""

import json
import tempfile
import unittest
from importlib import metadata
from pathlib import Path
from unittest.mock import patch

import polars as pl
from yamaa import _native as yamaa_native

import yaml

ROOT = Path(__file__).parent
SCHEMA = ROOT / "specification-yaml"


class InstalledLockedHostCapabilities(unittest.TestCase):
    def test_actual_yamaa_versions_repeat_without_importing_called_code(self):
        from yamaa import _locked_functions as host

        installed_version = metadata.version("yamaa")
        raw = (
            'version = 1\n[[package]]\nname = "yamaa"\n'
            f'version = "{installed_version}"\n'
        ).encode()
        with patch.object(
            host.importlib,
            "import_module",
            side_effect=AssertionError("lock verification must not import called code"),
        ):
            for _ in range(2):
                self.assertEqual(
                    host.verify_versions(raw, ["yamaa._native.engine_info"]), ()
                )
            wrong = raw.replace(
                f'version = "{installed_version}"'.encode(),
                b'version = "999999.0"',
            )
            self.assertEqual(
                host.verify_versions(wrong, ["yamaa._native.engine_info"]),
                (
                    host.Finding(
                        "yamaa", "version_mismatch", ("999999.0",), installed_version
                    ),
                ),
            )
        bound = host.resolve_callable("yamaa._native.engine_info", [])
        self.assertIs(bound, yamaa_native.engine_info)
        self.assertEqual(bound()["core_version"], "0.1.0")

    def test_normally_installed_project_wheel_is_checked_before_import_and_binding(
        self,
    ):
        import gc
        import io
        import subprocess
        import sys
        import zipfile

        from yamaa import _locked_functions as host

        module = "yamaa_lock_witness_programs"
        info = "yamaa_lock_witness_programs-1.2.0.dist-info"
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            marker = root / "project-imported.txt"
            target = root / "installed"
            wheel = root / "yamaa_lock_witness_programs-1.2.0-py3-none-any.whl"
            source = (
                "from pathlib import Path\n"
                f"Path({str(marker)!r}).write_text('imported', encoding='utf-8')\n"
                "calls = []\n"
                "failure = None\n"
                "failure_after = 0\n"
                "def calculate(*, x, y=999):\n"
                "    calls.append((x, y))\n"
                "    if failure is not None and len(calls) > failure_after: raise failure\n"
                "    return x + y\n"
            )
            members = {
                f"{module}/__init__.py": source,
                f"{info}/METADATA": "Metadata-Version: 2.1\nName: yamaa-lock-witness-programs\nVersion: 1.2.0\n",
                f"{info}/WHEEL": "Wheel-Version: 1.0\nGenerator: yamaa-installed-host-witness\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
                f"{info}/top_level.txt": module + "\n",
            }
            record = f"{info}/RECORD"
            members[record] = "".join(name + ",,\n" for name in [*members, record])
            with zipfile.ZipFile(wheel, "w") as archive:
                for name, content in members.items():
                    archive.writestr(name, content)
            installed = subprocess.run(
                [
                    "uv",
                    "pip",
                    "install",
                    "--python",
                    sys.executable,
                    "--target",
                    str(target),
                    "--no-cache",
                    "--offline",
                    "--no-deps",
                    str(wheel),
                ],
                check=False,
                capture_output=True,
                text=True,
                timeout=120,
            )
            self.assertEqual(
                installed.returncode, 0, installed.stdout + installed.stderr
            )
            raw = (
                'version = 1\n[[package]]\nname = "yamaa"\n'
                f'version = "{metadata.version("yamaa")}"\n'
                '[[package]]\nname = "yamaa-lock-witness-programs"\nversion = "1.2"\n'
            ).encode()
            self.assertNotIn(module, sys.modules)
            with patch.object(sys, "path", [str(target), *sys.path]):
                try:
                    bad = raw.replace(b'"1.2"', b'"9.0"')
                    self.assertEqual(
                        host.verify_versions(bad, [module + ".calculate"]),
                        (
                            host.Finding(
                                "yamaa-lock-witness-programs",
                                "version_mismatch",
                                ("9.0",),
                                "1.2.0",
                            ),
                        ),
                    )
                    self.assertFalse(marker.exists())
                    self.assertNotIn(module, sys.modules)
                    for _ in range(2):
                        self.assertEqual(
                            host.verify_versions(raw, [module + ".calculate"]), ()
                        )
                    self.assertFalse(marker.exists())
                    bound = host.resolve_callable(module + ".calculate", ["x", "y"])
                    self.assertEqual(marker.read_text(encoding="utf-8"), "imported")
                    self.assertEqual(sys.modules[module].calls, [])
                    self.assertEqual(bound(x=1, y=100), 101)
                    self.assertEqual(sys.modules[module].calls, [(1, 100)])
                    # The installed native project handle consumes the same
                    # normally installed wheel, held candidate schema bytes and
                    # approved file resources. No host planner selects calls.
                    schema_names = (
                        "schema.yaml",
                        "schema_shared.yaml",
                        "schema_derivation.yaml",
                        "schema_verification.yaml",
                        "schema_metadata.yaml",
                        "schema_function.yaml",
                        "schema_expression_core.yaml",
                        "schema_expression_aggregate.yaml",
                        "schema_expression_numeric.yaml",
                        "schema_expression_str.yaml",
                        "schema_expression_date.yaml",
                        "schema_expression_mapping.yaml",
                        "schema_expression_window.yaml",
                        "schema_expression_odm.yaml",
                    )
                    spec_modules = [
                        (name, (SCHEMA / name).read_bytes()) for name in schema_names
                    ]
                    env_root = ROOT / "specification-environment-candidate"
                    env_modules = [
                        (name, (env_root / name).read_bytes())
                        for name in ("schema_environment.yaml", "schema_shared.yaml")
                    ]
                    (root / "uv.lock").write_bytes(raw)
                    (root / "environment.yaml").write_text(
                        "schema_version: '1.0'\nlanguage: python\nlock: uv.lock\nfunctions:\n"
                        "  add:\n    function: yamaa_lock_witness_programs.calculate\n"
                        "    description: Add a declared default.\n"
                        "    params: [{name: x, type: int}, {name: y, type: int, required: false, default: 100}]\n"
                        "    returns: int\n    tests:\n"
                        "      - {id: normal, covers: [normal, 'default:y'], args: {x: 1}, result: 101}\n"
                        "      - {id: boundary, covers: [boundary], args: {x: -9223372036854775808}, result: -9223372036854775708}\n"
                        "      - {id: missing, covers: ['short-circuit-missing:x'], args: {x: null}, result: null}\n"
                        "      - {id: missing-y, covers: ['short-circuit-missing:y'], args: {x: 1, y: null}, result: null}\n",
                        encoding="ascii",
                    )
                    (root / "domain.yaml").write_text(
                        "schema_version: '1.0'\ndomain: TEST\nkeys: [x]\n"
                        "input: {SRC: {path: input.csv, types: {x: int}}}\n"
                        "columns:\n  - {name: x, type: int, derivation: SRC.x}\n"
                        "  - {name: V, type: int, derivation: {function: {name: add, args: {x: SRC.x}}}}\n"
                        "output: {path: result.csv, columns: [x, V]}\n",
                        encoding="ascii",
                    )
                    (root / "input.csv").write_bytes(b"x\n1\n")
                    sys.modules.pop(module)
                    marker.unlink()
                    native_project = yamaa_native._prepare_file_project(
                        str(root),
                        str(root),
                        "domain.yaml",
                        "environment.yaml",
                        [],
                        spec_modules,
                        0,
                        env_modules,
                        0,
                    )
                    self.assertEqual(native_project.preparation_status(), "ready")
                    self.assertNotIn(module, sys.modules)
                    self.assertFalse(marker.exists())
                    first = native_project.build()
                    self.assertTrue(first.engine_succeeded())
                    self.assertEqual(first.study_snapshot(0), b"x\n1\n")
                    evidence = json.loads(first.activation())
                    self.assertEqual(evidence["lock"], "verified")
                    self.assertEqual(
                        [test["outcome"] for test in evidence["tests"]], ["passed"] * 4
                    )
                    self.assertEqual(
                        evidence["tests"][1]["actual"], {"int": "-9223372036854775708"}
                    )
                    self.assertEqual(evidence["tests"][2]["actual"], {"missing": None})
                    self.assertFalse(evidence["tests"][2]["invoked"])
                    self.assertEqual(first.host_failures(), [])
                    self.assertEqual(first.failure_facts(), [])
                    self.assertEqual(
                        sys.modules[module].calls,
                        [(1, 100), (-9223372036854775808, 100), (1, 100)],
                    )
                    report_metadata = (
                        sys.version.split()[0],
                        "0.1.0",
                        "owned-project",
                        "domain.yaml",
                        str(root),
                    )
                    complete = first.result(report_metadata)
                    with self.assertRaisesRegex(ValueError, "invalid report metadata"):
                        first.result(())
                    self.assertEqual(complete.report_status(), "complete")
                    self.assertEqual(complete.issues(), [])
                    self.assertEqual(
                        pl.read_ipc_stream(io.BytesIO(complete.output())).to_dict(
                            as_series=False
                        ),
                        {"x": [1], "V": [101]},
                    )
                    published = []
                    saved = json.loads(
                        complete.save(
                            lambda path, content: published.append((path, content))
                        )
                    )
                    self.assertEqual(published, [("result.csv", b"x,V\n1,101\n")])
                    self.assertEqual(saved["outcome"], "success")
                    self.assertEqual(saved["artifacts"][0]["content"], "x,V\n1,101\n")
                    self.assertEqual(
                        sys.modules[module].calls,
                        [(1, 100), (-9223372036854775808, 100), (1, 100)],
                    )
                    prepared_sources = first.prepared_sources()
                    self.assertEqual(
                        [Path(identity).name for identity, _ in prepared_sources],
                        ["domain.yaml", "environment.yaml"],
                    )
                    self.assertEqual(
                        prepared_sources[0][1], (root / "domain.yaml").read_bytes()
                    )
                    self.assertEqual(
                        prepared_sources[1][1], (root / "environment.yaml").read_bytes()
                    )
                    held_reads = native_project.capture_reads()
                    (root / "uv.lock").write_bytes(bad)
                    second = native_project.build()
                    self.assertTrue(second.engine_succeeded())
                    self.assertEqual(second.study_snapshot(0), b"x\n1\n")
                    self.assertEqual(native_project.capture_reads(), held_reads)
                    self.assertEqual(len(sys.modules[module].calls), 6)
                    self.assertEqual(json.loads(first.activation()), evidence)
                    (root / "input.csv").write_bytes(b"x\n999\n")
                    changed = native_project.build()
                    self.assertFalse(changed.engine_succeeded())
                    self.assertEqual(first.study_snapshot(0), b"x\n1\n")
                    self.assertEqual(json.loads(first.activation()), evidence)
                    self.assertEqual(len(sys.modules[module].calls), 8)
                    rejected = yamaa_native._prepare_file_project(
                        str(root),
                        str(root),
                        "domain.yaml",
                        "environment.yaml",
                        [],
                        spec_modules,
                        0,
                        env_modules,
                        0,
                    )
                    before = rejected.capture_reads()
                    failed = rejected.build()
                    self.assertFalse(failed.engine_succeeded())
                    self.assertEqual(rejected.capture_reads(), before)
                    facts = failed.failure_facts()
                    self.assertEqual(facts[0][0], "lock")
                    self.assertEqual(
                        facts[0][1]["findings"][0]["reason"], "version_mismatch"
                    )
                    self.assertEqual(len(sys.modules[module].calls), 8)
                    rejected_result = failed.result(report_metadata)
                    self.assertEqual(rejected_result.report_status(), "complete")
                    self.assertIsNone(rejected_result.output())
                    self.assertTrue(rejected_result.issues())
                    with self.assertRaisesRegex(
                        ValueError, "cannot save a failed build"
                    ):
                        rejected_result.save(
                            lambda *_: self.fail("failed build published")
                        )
                    (root / "uv.lock").write_bytes(raw)
                    (root / "input.csv").write_bytes(b"x\n1\n")
                    for original, after, stage in (
                        (ValueError("original conformance failure"), 0, "conformance"),
                        (KeyboardInterrupt("original interrupt"), 0, "conformance"),
                        (ValueError("original live failure"), 2, "derivation"),
                    ):
                        package = sys.modules[module]
                        package.calls.clear()
                        package.failure, package.failure_after = original, after
                        checked = yamaa_native._prepare_file_project(
                            str(root),
                            str(root),
                            "domain.yaml",
                            "environment.yaml",
                            [],
                            spec_modules,
                            0,
                            env_modules,
                            0,
                        )
                        before = checked.capture_reads()
                        attempted = checked.build()
                        self.assertFalse(attempted.engine_succeeded())
                        failures = attempted.host_failures()
                        self.assertTrue(failures)
                        self.assertTrue(
                            all(
                                held_stage == stage and held is original
                                for held_stage, held in failures
                            )
                        )
                        self.assertTrue(
                            all(
                                facts["exception"] is original
                                for _, facts in attempted.failure_facts()
                            )
                        )
                        held_result = attempted.result(report_metadata)
                        del attempted
                        gc.collect()
                        self.assertTrue(
                            all(
                                held is original
                                for _, held in held_result.host_failures()
                            )
                        )
                        self.assertTrue(
                            all(
                                facts["exception"] is original
                                for _, facts in held_result.failure_facts()
                            )
                        )
                        if isinstance(original, KeyboardInterrupt):
                            self.assertEqual(
                                held_result.report_status(), "activation_boundary"
                            )
                            with self.assertRaises(KeyboardInterrupt) as caught:
                                held_result.propagate_interrupt()
                            self.assertIs(caught.exception, original)
                            with self.assertRaisesRegex(
                                ValueError, "original attempt remains retained"
                            ):
                                held_result.observations()
                        else:
                            self.assertEqual(held_result.report_status(), "complete")
                            self.assertIsNone(held_result.output())
                            issues = held_result.issues()
                            self.assertTrue(issues)
                            context = json.loads(issues[0][4])
                            if stage == "conformance":
                                self.assertEqual(
                                    issues[0][1], "function_conformance_failed"
                                )
                                context = context["failure"]["context"]
                            self.assertEqual(context["host_error"], "ValueError")
                            self.assertEqual(context["host_message"], str(original))
                            with self.assertRaisesRegex(
                                ValueError, "cannot save a failed build"
                            ):
                                held_result.save(
                                    lambda *_: self.fail("failed attempt published")
                                )
                        if stage == "conformance":
                            self.assertEqual(checked.capture_reads(), before)
                        else:
                            self.assertEqual(held_result.study_snapshot(0), b"x\n1\n")
                            self.assertEqual(
                                [
                                    test["outcome"]
                                    for test in json.loads(held_result.activation())[
                                        "tests"
                                    ]
                                ],
                                ["passed"] * 4,
                            )
                        if isinstance(original, KeyboardInterrupt):
                            self.assertEqual(
                                len(json.loads(held_result.activation())["tests"]), 1
                            )
                            self.assertEqual(len(package.calls), 1)

                    # Error rendering may execute host code. Reentrant reads
                    # refuse promptly while the same owned attempt is borrowed.
                    reentries = []
                    rendering_holder = []

                    class ReentrantError(ValueError):
                        def __str__(error):
                            with self.assertRaisesRegex(
                                RuntimeError, "already borrowed"
                            ) as caught:
                                rendering_holder[0].host_failures()
                            reentries.append(caught.exception)
                            return "retained reentrant failure"

                    reentrant = ReentrantError()
                    package.calls.clear()
                    package.failure, package.failure_after = reentrant, 0
                    checked = yamaa_native._prepare_file_project(
                        str(root),
                        str(root),
                        "domain.yaml",
                        "environment.yaml",
                        [],
                        spec_modules,
                        0,
                        env_modules,
                        0,
                    )
                    rendered_attempt = checked.build()
                    rendering_holder.append(rendered_attempt)
                    rendered_result = rendered_attempt.result(report_metadata)
                    self.assertEqual(rendered_result.report_status(), "complete")
                    self.assertTrue(reentries)
                    del rendered_attempt
                    rendering_holder.clear()
                    gc.collect()
                    self.assertTrue(
                        all(
                            held is reentrant
                            for _, held in rendered_result.host_failures()
                        )
                    )
                    self.assertIn(
                        "retained reentrant failure", rendered_result.observations()
                    )

                    # Refusing a complete aggregate report cannot discard any
                    # actual failure or keep only the prefix that fit the quota.
                    environment = yaml.safe_load(
                        (root / "environment.yaml").read_text(encoding="ascii")
                    )
                    definition = environment["functions"]["add"]
                    definition["tests"].extend(
                        {
                            "id": f"quota{index}",
                            "covers": ["normal"],
                            "args": {"x": 1},
                            "result": 101,
                        }
                        for index in range(358)
                    )
                    (root / "wide-environment.yaml").write_text(
                        yaml.safe_dump(environment), encoding="ascii"
                    )
                    large_error = ValueError("x" * 8192)
                    package.calls.clear()
                    package.failure, package.failure_after = large_error, 0
                    wide = yamaa_native._prepare_file_project(
                        str(root),
                        str(root),
                        "domain.yaml",
                        "wide-environment.yaml",
                        [],
                        spec_modules,
                        0,
                        env_modules,
                        0,
                    )
                    self.assertEqual(wide.preparation_status(), "ready")
                    before = wide.capture_reads()
                    wide_attempt = wide.build()
                    self.assertEqual(wide.capture_reads(), before)
                    self.assertEqual(len(wide_attempt.host_failures()), 360)
                    limited = wide_attempt.result(report_metadata)
                    self.assertEqual(limited.report_status(), "report_limit")
                    del wide_attempt, wide
                    gc.collect()
                    self.assertEqual(len(limited.host_failures()), 360)
                    self.assertTrue(
                        all(held is large_error for _, held in limited.host_failures())
                    )
                    with self.assertRaisesRegex(
                        ValueError, "original attempt remains retained"
                    ):
                        limited.save(lambda *_: self.fail("refused report published"))
                    (root / "domain.yaml").write_bytes(b"[changed")
                    (root / "environment.yaml").write_bytes(b"[changed")
                    del native_project
                    self.assertEqual(first.prepared_sources(), prepared_sources)
                    self.assertEqual(json.loads(first.activation()), evidence)
                    self.assertEqual(first.study_snapshot(0), b"x\n1\n")
                    del first
                    gc.collect()
                    self.assertEqual(complete.prepared_sources(), prepared_sources)
                    self.assertEqual(json.loads(complete.activation()), evidence)
                    self.assertEqual(complete.study_snapshot(0), b"x\n1\n")
                    self.assertEqual(
                        pl.read_ipc_stream(io.BytesIO(complete.output())).to_dict(
                            as_series=False
                        ),
                        {"x": [1], "V": [101]},
                    )
                    complete.save_file("result.csv", str(root / "retained.csv"))
                    self.assertEqual(
                        (root / "retained.csv").read_bytes(), b"x,V\n1,101\n"
                    )
                    print(
                        "owned installed Python project result retains complete reports, exact saves and original failures after attempt collection passed"
                    )
                    print(
                        "owned installed project run retains fresh activation, cached source, i64/missing evidence and original host failures passed"
                    )
                finally:
                    sys.modules.pop(module, None)


if __name__ == "__main__":
    unittest.main()
