"""Independently replay retained public-performance/1 evidence, without engines.

See audit_public_performance.schema.json for input and item-10 budget records.
Original runner paths are identities only: all reads use retained relative paths.
The optional self-contained Git bundle preserves pinned source after checkout loss.
Correctness, complete 567-cell coverage and numeric acceptance are distinct gates.
"""

from __future__ import annotations

import argparse
import contextlib
import datetime
import email.parser
import fnmatch
import json
import math
import re
import statistics
import subprocess
import tarfile
import tempfile
import uuid
import zipfile
from pathlib import Path, PurePosixPath, PureWindowsPath

import admit_performance_native as native_admission
import application_measurement_evidence as phases
import child_peak_rss
import collect_public_performance as collector
import measure_public_lifecycle as lifecycle
import prepare_public_workloads as workloads
import tomllib

PROTOCOL = "public-performance-audit-input/1"
SAMPLING = {"cold": 10, "warm": 30, "warmups": 3}
SYSTEMS = {
    "ubuntu-24.04": ("Linux", "x86_64"),
    "macos-15": ("Darwin", "arm64"),
    "windows-latest": ("Windows", "AMD64"),
}
ROLES = ("candidate", "baseline", "producer-root-build-baseline")
EVIDENCE_ERRORS = (
    ValueError,
    KeyError,
    AttributeError,
    IndexError,
    OSError,
    TypeError,
    EOFError,
    tarfile.TarError,
    zipfile.BadZipFile,
    subprocess.CalledProcessError,
)


def tuples():
    return [
        {"host": "python", "os": os, "python": version, "form": form}
        for os in SYSTEMS
        for version in ("3.12", "3.13", "3.14")
        for form in ("wheel", "source")
    ] + [{"host": "r", "os": os, "python": None, "form": "source"} for os in SYSTEMS]


def case_names():
    return [
        f"{family}-{rows}-{dimension}"
        for family in workloads.FAMILIES
        for rows in workloads.DEFAULT_ROWS
        for dimension in workloads.DIMENSIONS[family]
    ]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def exact(value, keys, label):
    require(
        isinstance(value, dict) and set(value) == set(keys), label + " fields differ"
    )


def nonempty(value, label):
    require(isinstance(value, str) and bool(value.strip()), "missing " + label)


def relative(root, name):
    nonempty(name, "retained path")
    require(
        not PureWindowsPath(name).is_absolute() and not Path(name).is_absolute(),
        "retained path must be relative",
    )
    path = (root / name).resolve(strict=True)
    require(path.is_relative_to(root.resolve()), "retained path escapes evidence")
    return path


def original_path(value, system):
    nonempty(value, "original runner path")
    path = PureWindowsPath(value) if system == "Windows" else PurePosixPath(value)
    require(path.is_absolute(), "original runner path must be absolute")
    require(".." not in path.parts, "original runner path contains traversal")
    return path


def read(path):
    # Duplicate JSON keys can hide a changed original declaration or verdict.
    def pairs(items):
        value = {}
        for key, item in items:
            require(key not in value, "duplicate JSON member: " + key)
            value[key] = item
        return value

    return json.loads(
        lifecycle.retained_bytes(path),
        object_pairs_hook=pairs,
        parse_constant=lambda value: (_ for _ in ()).throw(ValueError(value)),
    )


def stats(values, unit):
    require(
        bool(values)
        and all(type(v) in (int, float) and math.isfinite(v) and v > 0 for v in values),
        "zero/nonfinite or empty observed metric",
    )
    return {
        "count": len(values),
        "raw": values,
        "unit": unit,
        "median": statistics.median(values),
        "min": min(values),
        "max": max(values),
        "quartiles": statistics.quantiles(values, n=4, method="inclusive"),
    }


def offline_origins(observed, declared, repository):
    """Same strict identities as actual_origins, without resolving vanished paths."""
    for key in (
        "host",
        "runtime_version",
        "package_version",
        "system",
        "installed_origins",
    ):
        require(
            observed[key] == declared[key], "actual installed identity differs: " + key
        )
    require(
        observed.get("engine_version") in (None, declared["engine_version"]),
        "actual core version differs",
    )
    runtime = observed["report_runtime_version"]
    nonempty(runtime, "complete-report runtime identity")
    require(
        (runtime.split()[0] if declared["host"] == "python" else runtime)
        == declared["runtime_version"],
        "complete-report runtime differs",
    )
    if declared["host"] == "python":
        clock = observed["clock"]
        require(
            clock["provider"] == "time.perf_counter_ns"
            and clock["unit"] == "ns"
            and clock["monotonic"] is True
            and clock["adjustable"] is False,
            "actual Python clock differs",
        )
    else:
        require(
            observed["collectors"] == declared["collectors"],
            "actual R collectors differ",
        )
        require(
            observed["clock_library"]
            == declared["clock_evidence"]["installed_library"],
            "actual R timer library differs",
        )
        require(
            observed["clock"]["provider"] == declared["clock_evidence"]["provider"]
            and observed["clock"]["api"] == "microbenchmark::get_nanotime"
            and observed["clock"]["unit"] == "ns",
            "actual R timer provider differs",
        )


def admit_installation(directory, plan, lane, role, repository, scratch):
    record = read(directory / "installation/declared.json")
    require(
        record == plan["provenance"],
        "collection differs from original installation declaration",
    )
    required = (
        "source_revision",
        "artifact_reference",
        "artifact_path",
        "package_form",
        "host",
        "runtime_version",
        "package_version",
        "engine_version",
        "toolchain",
        "installed_origins",
        "system",
        "cpu",
        "installation_evidence",
    )
    require(all(key in record for key in required), "missing installation identity")
    for key in (
        "source_revision",
        "artifact_reference",
        "runtime_version",
        "package_version",
        "engine_version",
    ):
        nonempty(record[key], key)
    require(
        record["source_revision"] == plan["source_revision"],
        "collection source differs",
    )
    require(
        record["engine_version"]
        == tomllib.loads(
            lifecycle.git_bytes(
                repository, record["source_revision"], "rust/Cargo.toml"
            ).decode("utf-8")
        )["workspace"]["package"]["version"],
        "core version differs from pinned source",
    )
    require(
        record["host"] == lane["host"] and record["system"] == SYSTEMS[lane["os"]][0],
        "installation tuple differs",
    )
    form = lane["form"] if role == "candidate" else "wheel"
    require(
        record["package_form"] == form,
        "package form differs from tuple/supported baseline",
    )
    require(
        role == "candidate"
        or (record["host"] == "python" and record["package_version"] == "0.3.0"),
        "baseline must be the approved prior Python 0.3.0 package",
    )
    runtime = lane["python"] + "." if lane["host"] == "python" else "4.6.1"
    require(
        record["runtime_version"].startswith(runtime)
        if lane["host"] == "python"
        else record["runtime_version"] == runtime,
        "runtime differs from approved tuple",
    )
    exact(record["installed_origins"], ("package", "native"), "installed origins")
    package, native = [
        original_path(record["installed_origins"][key], record["system"])
        for key in ("package", "native")
    ]
    require(native.is_relative_to(package), "native library outside installed package")
    installation = read(directory / "installation/installation.json")
    for key in ("source_revision", "artifact_reference", "package_form"):
        require(
            installation["evidence"][key] == record[key],
            "installation evidence differs: " + key,
        )
    for left, right in (
        ("runtime_version", "runtime_version"),
        ("package_version", "host_package_version"),
        ("engine_version", "core_version"),
    ):
        require(
            record[left] == installation["installation"][right],
            "installed version differs: " + left,
        )
    require(
        installation["installation"]["package_location"]
        == record["installed_origins"][
            "native" if lane["host"] == "python" else "package"
        ],
        "installation origin differs",
    )
    nonempty(installation["build_association"], "source/native build association")
    artifact = relative(
        directory / "installation",
        original_path(record["artifact_path"], record["system"]).name,
    )
    installed = directory / "installation/installed-native-library.bin"
    require(installed.stat().st_size > 0, "empty retained installed native library")
    relocated = dict(
        record,
        artifact_path=str(artifact),
        installed_origins={"native": str(installed)},
    )
    # This reads original archives/native bytes and pinned Git source; receipt booleans are ignored.
    lifecycle.artifact_bytes(relocated, repository)
    if record["package_form"] == "wheel":
        with zipfile.ZipFile(artifact) as wheel:
            metadata = [
                name
                for name in wheel.namelist()
                if name.endswith(".dist-info/METADATA")
            ]
            require(len(metadata) == 1, "wheel package metadata absent or ambiguous")
            package_metadata = email.parser.BytesParser().parsebytes(
                wheel.read(metadata[0])
            )
            require(
                package_metadata["Name"] == "yamaa"
                and package_metadata["Version"] == record["package_version"],
                "wheel package/version differs from installed identity",
            )
    if lane["host"] == "r":
        clock = directory / "installation/clock-build"
        require(
            (clock / "built-library.bin").read_bytes()
            == (clock / "installed-library.bin").read_bytes()
            and (clock / "built-library.bin").stat().st_size > 0,
            "retained R clock bytes differ",
        )
        # Reuse the original clock build admission against retained bytes in a private scratch tree.
        rebuilt = dict(
            record,
            collectors={
                key: dict(value) for key, value in record["collectors"].items()
            },
            clock_evidence=dict(record["clock_evidence"]),
        )
        for spec in rebuilt["collectors"].values():
            spec["origin"] = str(clock.resolve())
        rebuilt["clock_evidence"].update(
            build_config=str(clock / "build-config.txt"),
            built_library=str(clock / "built-library.bin"),
            installed_library=str(clock / "installed-library.bin"),
        )
        lifecycle.clock_evidence(rebuilt, repository, scratch / "clock-admission")
    return record


def regenerate(destination, case, repository, revision, scratch):
    """Replay generator with pinned original locks, even without a live checkout."""
    locks = scratch / "original-locks"
    for name in ("python/uv.lock", "benchmarks/schema-functions/r/renv.lock"):
        target = locks / name
        if not target.exists():
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(lifecycle.git_bytes(repository, revision, name))
    previous = workloads.REPOSITORY
    try:
        workloads.REPOSITORY = locks
        workloads.prepare(destination, (case["driving_rows"],), (case["family"],))
    finally:
        workloads.REPOSITORY = previous


def root_invocation(actual, request, record, repository):
    invocation = actual["build_invocation"]
    require(
        invocation["callable"] == "yamaa._reference_domain.yamaa_domain"
        and "environment" not in invocation["parameters"]
        and "schema_root" in invocation["parameters"]
        and invocation["schema_root"] == request["reference_schema"]
        and invocation["environment_passed"] is False,
        "root-Build invocation differs",
    )
    source = original_path(invocation["source_file"], record["system"])
    package = original_path(record["installed_origins"]["package"], record["system"])
    require(
        source.is_relative_to(package) and source.name == "_reference_domain.py",
        "root-Build callable source outside installed package",
    )
    artifact = Path(record["_retained_artifact"])
    with zipfile.ZipFile(artifact) as wheel:
        require(
            wheel.read("yamaa/_reference_domain.py")
            == lifecycle.git_bytes(
                repository,
                record["source_revision"],
                "python/src/yamaa/_reference_domain.py",
            ),
            "root-Build callable differs from pinned original source",
        )
    require(
        not actual["save_returns"]
        and not actual["saved_files"]
        and not actual.get("saved_artifacts")
        and not any(key.startswith("held_save_") for key in actual["durations"]),
        "root-Build baseline entered or timed Save",
    )
    require(
        not any(actual["root_build_artifact_presence"].values()),
        "root-Build published an artifact",
    )


def schedule(case, role):
    result = [("truth", 0, [{"index": 0, "warmup": False}])]
    if role == "candidate" and case["family"] == "callback":
        result.append(("callback-observer", 0, [{"index": 0, "warmup": False}]))
    result += [("cold", n, [{"index": n, "warmup": False}]) for n in range(10)]
    return result + [("warm", 0, [{"index": n, "warmup": n < 3} for n in range(33)])]


def admit_case(directory, plan, row, case, authored, record, role, repository):
    expected = schedule(case, role)
    require(
        len(row["children"]) == len(expected),
        "missing/duplicate/unexpected timing child",
    )
    require(
        {
            path.name
            for path in directory.iterdir()
            if path.is_dir() and path.name.startswith(case["name"] + "-")
        }
        == {case["name"] + f"-{mode}-{number}" for mode, number, _ in expected},
        "unexpected/unretained child attempt directory",
    )
    groups, callbacks, function_calls = {}, [], []
    unavailable = (
        list(lifecycle.unavailable(plan["phase_measurements_required"]))
        if role == "candidate"
        else []
    )
    ordinary = None
    for observation, (mode, number, declarations) in zip(row["children"], expected):
        name = case["name"] + f"-{mode}-{number}"
        require(
            observation["child"] == name and observation["mode"] == mode,
            "child order/identity differs from original schedule",
        )
        require(
            observation.get("failure") is None,
            "original failed attempt cannot become timing evidence",
        )
        child = relative(directory, name)
        request, response, process = [
            read(child / filename)
            for filename in ("request.json", "response.json", "process.json")
        ]
        for filename in ("stdout.txt", "stderr.txt"):
            require((child / filename).is_file(), "missing raw child transport member")
        probe = read(child / "compilation-probe.json")
        require(
            probe["provider"] == "process-name inventory"
            and probe["compilers"] == []
            and probe["clear"] is True
            and probe["atomic_lock"] is False,
            "compilation active or unobserved",
        )
        require(
            request["protocol"] == lifecycle.PROTOCOL
            and response["protocol"] == lifecycle.PROTOCOL
            and request["child"] == name
            and response["child"] == name,
            "raw worker protocol/identity differs",
        )
        require(
            request["provenance"] == plan["provenance"]
            and request["host"] == plan["host"]
            and request["evaluator"] == plan["evaluator"],
            "worker source/installation/evaluator differs",
        )
        require(
            request["phase_measurements"] == plan["phase_measurements_required"]
            and request["observe_callbacks"] == (mode == "callback-observer")
            and request["reference_root_build"]
            == (role == "producer-root-build-baseline"),
            "worker measurement scope differs",
        )
        require(
            process["exit_code"] == 0
            and type(process["pid"]) is int
            and process["pid"] > 0
            and process["pid"] == response["pid"],
            "failed child/PID association",
        )
        require(
            process["launcher"] == "fresh small broker; controller fork RSS excluded",
            "original child RSS broker association differs",
        )
        child_peak_rss.validate(process["peak_rss"], record["system"])
        require(
            math.isfinite(process["peak_rss"]["bytes"])
            and process["peak_rss"]["bytes"] > 0,
            "nonfinite/zero actual child peak RSS",
        )
        require(
            len(request["repetitions"]) == len(declarations),
            "missing retained warmup/repetition",
        )
        original_child = original_path(request["output"], record["system"])
        require(original_child.name == name, "worker output identity differs")
        original_collection = original_path(
            plan["measurement_campaign"]["collection"], record["system"]
        )
        require(
            original_collection.name == "collection.json"
            and original_child == original_collection.parent / name,
            "worker belongs to another campaign collection",
        )
        require(
            request["reference_schema"] is None
            if role == "candidate"
            else original_path(request["reference_schema"], record["system"])
            == original_collection.parent / "reference-schema",
            "original reference schema scope differs",
        )
        for declaration, expected_declaration in zip(
            request["repetitions"], declarations
        ):
            require(
                {key: declaration[key] for key in ("index", "warmup")}
                == expected_declaration,
                "warmup/repetition schedule differs",
            )
            require(
                type(declaration["index"]) is int
                and type(declaration["warmup"]) is bool,
                "repetition index/warmup types differ",
            )
            work_name = f"work-{declaration['index']}"
            original_work = original_path(
                declaration["work_directory"], record["system"]
            )
            require(
                original_work == original_child / work_name,
                "repetition work identity differs",
            )
            require(
                declaration["entry"]
                == lifecycle.specification_identity(original_work / case["entry"]),
                "declaring-file identity differs",
            )
            environment = (
                lifecycle.specification_identity(
                    original_work / case["environment"][plan["host"]]
                )
                if case["environment"]
                else None
            )
            require(
                declaration["environment"] == environment
                and declaration["output_name"] == "out.csv"
                and declaration["artifact_names"]
                == [a["path"] for a in case["expected_artifacts"]],
                "worker environment/artifact declaration differs",
            )
            for member in case["files"]:
                if not member["path"].startswith("expected/"):
                    require(
                        relative(child, work_name + "/" + member["path"]).read_bytes()
                        == (authored / member["path"]).read_bytes(),
                        "original input/specification bytes changed",
                    )
        for repetition in response["repetitions"]:
            require(repetition["failure"] is None, "original raw repetition failed")
            lifecycle.assert_equal(
                repetition["host_view_access_counts"],
                {"output": 2, "issues": 1, "verification_log": 1, "warning_log": 1},
                "actual host getter counts",
            )
            for group in ("reports", "measurements"):
                for filename in repetition.get(group, {}).values():
                    relative(child, filename)
            relative(child, repetition["views"])
            for stage in repetition.get("saved_artifacts", {}).values():
                for filename in stage.values():
                    relative(child, filename)
        raw_record = dict(
            record,
            _retained_artifact=str(
                directory
                / "installation"
                / original_path(record["artifact_path"], record["system"]).name
            ),
        )
        if role == "candidate":
            collector.scalable.validate(
                case,
                authored,
                child,
                request,
                response,
                record,
                repository,
                origin_validator=offline_origins,
            )
        else:
            collector.validate_reference(
                case,
                authored,
                child,
                request,
                response,
                raw_record,
                repository,
                origin_validator=offline_origins,
                root_invocation_validator=root_invocation,
            )
        if mode == "truth":
            ordinary = (child, response["repetitions"][0])
        if mode == "callback-observer":
            actual = response["repetitions"][0]["callback_observation"]
            output = read(authored / case["expected_output"])
            values = [workloads.typed(7.25, "float"), workloads.typed(0.0, "float")]
            values += [
                cells[1] for cells in output["rows"] if cells[1]["type"] != "missing"
            ]
            trace = [
                {"parameters": {"x": value}, "result": value, "failure": None}
                for value in values
            ]
            require(
                actual["count"] == len(trace) and actual["trace"] == trace,
                "actual callback trace differs",
            )
            for stage in ("build", "save_1", "save_2"):
                lhs = collector.normalized_report(
                    ordinary[0] / ordinary[1]["reports"][stage],
                    original_path(
                        ordinary[1]["declaration"]["work_directory"], record["system"]
                    ),
                )
                rhs = collector.normalized_report(
                    child / response["repetitions"][0]["reports"][stage],
                    original_path(
                        request["repetitions"][0]["work_directory"], record["system"]
                    ),
                )
                require(lhs == rhs, "callback observer changed original public reports")
            callbacks.append(actual["count"])
        if mode not in ("cold", "warm"):
            continue
        groups.setdefault(mode + ".child_peak_rss", []).append(
            process["peak_rss"]["bytes"]
        )
        if mode == "cold":
            groups.setdefault("cold.package_import", []).append(
                lifecycle.validate_duration(response["import_duration"], plan["host"])
            )
        boundaries = [
            boundary
            for boundary in lifecycle.BOUNDARIES[1:]
            if role != "producer-root-build-baseline"
            or not boundary.startswith("held_save_")
        ]
        for repetition in response["repetitions"]:
            require(
                set(repetition["durations"]) == set(boundaries),
                "missing/unexpected timing boundary",
            )
            measurement = None
            if request["phase_measurements"]:
                measurement = phases.validate_repetition(case, child, repetition, read)
                function_calls.append(
                    {
                        "child": name,
                        "mode": mode,
                        "index": repetition["declaration"]["index"],
                        "warmup": repetition["declaration"]["warmup"],
                        "calls": measurement["function_calls"],
                    }
                )
            if repetition["declaration"]["warmup"]:
                continue
            for boundary in boundaries:
                duration = lifecycle.validate_duration(
                    repetition["durations"][boundary], plan["host"]
                )
                groups.setdefault(mode + "." + boundary, []).append(duration)
            if measurement is not None:
                totals = {}
                for stage in measurement["stages"]:
                    totals[stage["phase"]] = (
                        totals.get(stage["phase"], 0) + stage["duration_ns"]
                    )
                for phase, duration in totals.items():
                    groups.setdefault(mode + ".phase." + phase, []).append(duration)
                for number, save in enumerate(measurement["saves"], 1):
                    groups.setdefault(mode + f".phase.held_save_{number}", []).append(
                        save["completed_ns"] - save["entered_ns"]
                    )
    metrics = {}
    for name, values in groups.items():
        count = (
            1
            if name == "warm.child_peak_rss"
            else (10 if name.startswith("cold.") else 30)
        )
        require(len(values) == count, "timing distribution count differs")
        metrics[
            name
            + (".median_bytes" if name.endswith("child_peak_rss") else ".median_ns")
        ] = (
            stats(values, "bytes" if name.endswith("child_peak_rss") else "nanoseconds")
            if count > 1
            else {
                "count": 1,
                "raw": values,
                "unit": "bytes",
                "median": values[0],
                "min": values[0],
                "max": values[0],
                "quartiles": None,
            }
        )
    return {
        "status": "pass",
        "metrics": metrics,
        "unavailable": unavailable,
        "actual_callback_counts": callbacks,
        "actual_function_calls": function_calls,
    }


def admit_collection(
    directory, lane, role, source_revision, campaign, repository, scratch
):
    plan = read(directory / "collection.json")
    require(
        plan["protocol"] == "public-performance/1"
        and plan["purpose"] == "characterization",
        "frontier/correctness smoke cannot be timing evidence",
    )
    measurement_lane = (
        lane
        if role == "candidate"
        else dict(lane, host="python", python=lane["python"] or "3.12", form="wheel")
    )
    require(
        plan["host"] == measurement_lane["host"] and plan["sampling"] == SAMPLING,
        "collection host/sampling differs",
    )
    require(
        type(plan["phase_measurements_required"]) is bool,
        "phase measurement flag type differs",
    )
    require(
        plan["evaluator"] == ("public" if role == "candidate" else "python-reference")
        and plan["measurement_scope"]
        == (
            "producer-root-build"
            if role == "producer-root-build-baseline"
            else "full-lifecycle"
        ),
        "candidate/baseline scope differs",
    )
    require(
        role != "candidate" or plan["source_revision"] == source_revision,
        "candidate source differs from selected run",
    )
    receipt = plan["measurement_campaign"]
    require(
        campaign["protocol"] == "public-performance-campaign/1"
        and str(uuid.UUID(campaign["campaign_id"])) == campaign["campaign_id"],
        "invalid campaign identity",
    )
    require(
        receipt["protocol"] == campaign["protocol"]
        and receipt["campaign_id"] == campaign["campaign_id"]
        and receipt["measurement_machine"] == campaign["measurement_machine"]
        and receipt["role"] == role
        and receipt["source_revision"] == plan["source_revision"]
        and campaign["collections"].count(receipt) == 1,
        "campaign receipt/source/role differs",
    )
    machine = campaign["measurement_machine"]
    require(
        machine["system"] == SYSTEMS[lane["os"]][0]
        and machine["architecture"] == SYSTEMS[lane["os"]][1]
        and machine["node"]
        and type(machine["logical_cpus"]) is int
        and machine["logical_cpus"] > 0,
        "campaign actual machine differs from tuple",
    )
    require(
        original_path(receipt["collection"], machine["system"]).parent.name
        == directory.name,
        "campaign collection identity differs",
    )
    require(
        len(campaign["collections"])
        == len({r["collection"] for r in campaign["collections"]}),
        "duplicate campaign collection receipt",
    )
    record = admit_installation(
        directory, plan, measurement_lane, role, repository, scratch
    )
    if role != "candidate":
        source_paths = lifecycle.git_paths(
            repository, record["source_revision"], "yaml"
        )
        require(bool(source_paths), "original reference schema source absent")
        expected_paths = {
            Path(path).relative_to("yaml").as_posix() for path in source_paths
        }
        schema_root = relative(directory, "reference-schema")
        require(
            {
                path.relative_to(schema_root).as_posix()
                for path in schema_root.rglob("*")
                if path.is_file()
            }
            == expected_paths,
            "missing/unexpected original reference schema member",
        )
        for path in source_paths:
            require(
                relative(
                    schema_root, Path(path).relative_to("yaml").as_posix()
                ).read_bytes()
                == lifecycle.git_bytes(repository, record["source_revision"], path),
                "original reference schema bytes differ",
            )
    harness = directory / "harness"
    for member in harness.iterdir():
        if member.is_file():
            require(
                member.read_bytes()
                == lifecycle.git_bytes(
                    repository,
                    plan["harness_checkout_head"],
                    "rust/tools/" + member.name,
                ),
                "retained harness differs from pinned source",
            )
    expected_harness = {
        "collect_public_performance.py",
        "child_peak_rss.py",
        "measure_public_lifecycle.py",
        "measure_scalable_public.py",
        "prepare_public_workloads.py",
        "public_lifecycle_worker_protocol.json",
        "application_measurement_evidence.py",
        *lifecycle.WORKERS.values(),
    }
    require(
        expected_harness.issubset({p.name for p in harness.iterdir()}),
        "missing raw harness member",
    )
    manifest = read(directory / "workloads/manifest.json")
    require(
        manifest["protocol"] == "scalable-public-workloads/1"
        and manifest["sizes"] == list(workloads.DEFAULT_ROWS)
        and manifest["sampling"] == SAMPLING
        and [c["name"] for c in manifest["cases"]] == case_names(),
        "unexpected/duplicate/absent approved generator inventory",
    )
    require(
        (directory / "workloads/independent-generator.py").read_bytes()
        == Path(workloads.__file__).read_bytes(),
        "independent generator differs from auditor's reviewed generator",
    )
    for key, inventory in (
        ("selected_cases", case_names()),
        ("selected_families", workloads.FAMILIES),
    ):
        selected_names = plan[key]
        require(
            selected_names is None
            or (
                isinstance(selected_names, list)
                and bool(selected_names)
                and len(selected_names) == len(set(selected_names))
                and all(name in inventory for name in selected_names)
            ),
            "unexpected/duplicate/empty workload selection",
        )
    selected = [
        c
        for c in manifest["cases"]
        if (not plan["selected_cases"] or c["name"] in plan["selected_cases"])
        and (not plan["selected_families"] or c["family"] in plan["selected_families"])
        and (role != "producer-root-build-baseline" or c["family"] == "producer")
    ]
    require(
        bool(selected)
        and [r["case"] for r in plan["cases"]] == [c["name"] for c in selected],
        "duplicate/absent/empty/unexpected selected case coverage",
    )
    result = {}
    for row, case in zip(plan["cases"], selected):
        try:
            replay = scratch / (case["family"] + "-" + str(case["driving_rows"]))
            if not replay.exists():
                regenerate(
                    replay, case, repository, plan["truth_repository_head"], scratch
                )
            regenerated = read(replay / "manifest.json")
            declared = next(
                c for c in regenerated["cases"] if c["name"] == case["name"]
            )
            require(
                case == declared, "declared workload differs from independent generator"
            )
            authored = replay / case["name"]
            for member in case["files"]:
                require(
                    relative(
                        directory / "workloads" / case["name"], member["path"]
                    ).read_bytes()
                    == (authored / member["path"]).read_bytes(),
                    "altered expected/source workload bytes",
                )
            result[case["name"]] = dict(
                admit_case(
                    directory, plan, row, case, authored, record, role, repository
                ),
                measurement_host=plan["host"],
                source_revision=plan["source_revision"],
            )
        except EVIDENCE_ERRORS as error:
            result[case["name"]] = {"status": "fail", "reason": str(error)}
    return result


def selector(value, lane, case):
    return (value["tuple"] == "*" or value["tuple"] == lane) and (
        value["case"] == "*" or value["case"] == case
    )


def validate_budgets(value, source_revision):
    exact(
        value,
        ("protocol", "status", "source_revision", "decision", "rules", "dispositions"),
        "budget",
    )
    require(
        value["protocol"] == "public-performance-budgets/1"
        and value["status"] in ("approved", "unapproved"),
        "unsupported budget protocol/status",
    )
    require(value["source_revision"] == source_revision, "budget source differs")
    exact(value["decision"], ("issue", "item", "reference"), "budget decision")
    require(
        value["decision"]["issue"] == 1973 and value["decision"]["item"] == 10,
        "budget needs item-10 decision",
    )
    nonempty(value["decision"]["reference"], "budget decision reference")
    for group, fields in (
        ("rules", ("tuple", "case", "metric", "comparison", "limit")),
        (
            "dispositions",
            ("tuple", "case", "metric", "decision", "reason", "reference"),
        ),
    ):
        require(isinstance(value[group], list), "budget group must be a list")
        for rule in value[group]:
            exact(rule, fields, group)
            require(
                rule["tuple"] == "*" or rule["tuple"] in tuples(),
                "budget tuple outside approved matrix",
            )
            require(
                rule["case"] == "*" or rule["case"] in case_names(),
                "budget case outside approved inventory",
            )
            nonempty(rule["metric"], "budget metric")
            if group == "rules":
                require(
                    rule["metric"]
                    in read(Path(__file__).with_suffix(".schema.json"))["$defs"][
                        "rule"
                    ]["properties"]["metric"]["enum"],
                    "unknown budget metric",
                )
                require(
                    rule["comparison"] in ("absolute", "baseline-ratio"),
                    "unknown budget comparison",
                )
                require(
                    type(rule["limit"]) in (int, float)
                    and math.isfinite(rule["limit"])
                    and rule["limit"] > 0,
                    "budget threshold must be explicit positive finite number",
                )
            else:
                require(
                    rule["decision"] in ("unavailable", "incomparable"),
                    "unknown item-10 disposition",
                )
                nonempty(rule["reason"], "disposition reason")
                nonempty(rule["reference"], "disposition reference")
    return value


def evaluate_budgets(budgets, cells):
    if budgets is None or budgets["status"] != "approved":
        return {
            "status": "not_evaluated",
            "reason": "explicit approved item-10 thresholds absent",
            "results": [],
        }
    results = []
    for cell in cells:
        candidate = cell["candidate"]
        if candidate["status"] != "pass":
            results.append(
                {
                    "tuple": cell["tuple"],
                    "case": cell["case"],
                    "metric": "candidate",
                    "status": "incomplete",
                }
            )
            continue
        baseline = cell["baseline"]
        gaps = list(candidate["unavailable"])
        metrics = candidate["metrics"]
        for metric, distribution in metrics.items():
            comparable = (
                cell["tuple"]["host"] == "python"
                and cell["case"].split("-")[0] in ("lookup", "aggregate", "window")
                and baseline["status"] == "pass"
                and metric in baseline["metrics"]
            )
            if not comparable:
                gaps.append("baseline:" + metric)
            rules = [
                r
                for r in budgets["rules"]
                if selector(r, cell["tuple"], cell["case"]) and r["metric"] == metric
            ]
            status, measured = "incomplete", distribution["median"]
            if len(rules) == 1:
                rule = rules[0]
                if rule["comparison"] == "absolute":
                    status = "pass" if measured <= rule["limit"] else "fail"
                elif comparable:
                    measured /= baseline["metrics"][metric]["median"]
                    status = "pass" if measured <= rule["limit"] else "fail"
            results.append(
                {
                    "tuple": cell["tuple"],
                    "case": cell["case"],
                    "metric": metric,
                    "status": status,
                    "value": measured,
                    "rules": rules,
                }
            )
        for gap in gaps:
            dispositions = [
                d
                for d in budgets["dispositions"]
                if selector(d, cell["tuple"], cell["case"])
                and fnmatch.fnmatchcase(gap, d["metric"])
            ]
            results.append(
                {
                    "tuple": cell["tuple"],
                    "case": cell["case"],
                    "metric": gap,
                    "status": "pass" if len(dispositions) == 1 else "incomplete",
                    "dispositions": dispositions,
                }
            )
    statuses = {r["status"] for r in results}
    return {
        "status": "fail"
        if "fail" in statuses
        else "incomplete"
        if "incomplete" in statuses
        else "pass",
        "decision": budgets["decision"],
        "results": results,
    }


def validate_run(run, payload):
    require(
        run["head_sha"] == payload["source_revision"]
        and type(run["id"]) is int
        and run["id"] > 0
        and type(run["run_attempt"]) is int
        and run["run_attempt"] > 0
        and run["path"] == ".github/workflows/performance.yml",
        "selected performance source/run differs",
    )
    require(
        run["repository"]["full_name"] == payload["repository"]
        and run["head_repository"]["full_name"] == payload["repository"]
        and run["event"] in ("push", "workflow_dispatch", "workflow_call"),
        "untrusted/sampled run source",
    )
    # The current audit job can still be running. Its original named producers
    # must already have succeeded; terminal failed/cancelled attempts are refused.
    require(
        (run["status"] == "completed" and run["conclusion"] == "success")
        or (run["status"] == "in_progress" and run["conclusion"] is None),
        "failed/cancelled/incomplete performance attempt",
    )


def admit_artifact(entry, artifacts, run, jobs):
    lane = entry["tuple"]
    require(
        entry["run_attempt"] == run["run_attempt"],
        "artifact belongs to another attempt",
    )
    matches = [a for a in artifacts if a["id"] == entry["artifact_id"]]
    require(len(matches) == 1, "missing/duplicate original artifact identity")
    artifact = matches[0]
    name = (
        f"performance-python-{lane['os']}-{lane['python']}-{lane['form']}"
        if lane["host"] == "python"
        else f"performance-r-{lane['os']}-source"
    )
    require(
        artifact["name"] == name == entry["artifact_name"]
        and artifact["expired"] is False
        and type(artifact["id"]) is int
        and artifact["id"] > 0
        and artifact["size_in_bytes"] > 0
        and artifact["workflow_run"]["id"] == run["id"]
        and artifact["workflow_run"]["head_sha"] == run["head_sha"],
        "wrong/empty/expired original artifact",
    )
    require(
        len([a for a in artifacts if a["name"] == name]) == 1,
        "ambiguous artifact names from different attempts",
    )
    job_name = (
        f"performance-python ({lane['os']}, {lane['python']}, {lane['form']})"
        if lane["host"] == "python"
        else f"performance-r ({lane['os']}, source)"
    )
    matches = [job for job in jobs if job["name"].endswith(job_name)]
    require(len(matches) == 1, "missing/duplicate original producing job")
    job = matches[0]
    require(
        job["run_id"] == run["id"]
        and job["run_attempt"] == run["run_attempt"]
        and job["head_sha"] == run["head_sha"]
        and job["status"] == "completed"
        and job["conclusion"] == "success",
        "producing job source/attempt failed or incomplete",
    )
    parse_time = datetime.datetime.fromisoformat
    require(
        parse_time(job["started_at"])
        <= parse_time(artifact["created_at"])
        <= parse_time(job["completed_at"]),
        "artifact timestamp outside producing job attempt",
    )
    return artifact, job


def native_inputs(payload, root, performance_run):
    keys = ("native_run", "native_jobs", "native_artifacts")
    if not all(key in payload for key in keys):
        return {
            "status": "incomplete",
            "reason": "original native source/run/attempt/artifact metadata absent",
        }
    try:
        run, jobs, artifacts = [read(relative(root, payload[key])) for key in keys]
        jobs = jobs["jobs"] if isinstance(jobs, dict) else jobs
        artifacts = artifacts["artifacts"] if isinstance(artifacts, dict) else artifacts
        nonempty(performance_run["head_branch"], "original source branch")
        # Reuse #2137's full native admission, including exact producing-job
        # attempts and original artifact creation intervals.
        native_admission.validate(
            run,
            jobs,
            artifacts,
            payload["repository"],
            payload["source_revision"],
            performance_run["head_branch"],
        )
        ids = [item["id"] for item in artifacts]
        require(
            all(type(value) is int and value > 0 for value in ids)
            and len(ids) == len(set(ids)),
            "duplicate/invalid original native artifact ID",
        )
        return {"status": "pass", "run": run, "jobs": jobs, "artifacts": artifacts}
    except EVIDENCE_ERRORS as error:
        return {"status": "fail", "reason": str(error)}


def native_collection(directory, lane, native):
    require(
        native["status"] == "pass",
        native.get("reason", "native source admission incomplete"),
    )
    original = read(relative(directory.parent, "native-run.json"))
    for key in (
        "id",
        "run_attempt",
        "head_sha",
        "head_branch",
        "path",
        "event",
        "status",
        "conclusion",
        "repository",
        "head_repository",
    ):
        lifecycle.assert_equal(
            original[key], native["run"][key], "original native run/attempt: " + key
        )
    name = (
        f"native-python-{lane['os']}-3.14"
        if lane["host"] == "python"
        else f"native-r-{lane['os']}"
    )
    (artifact,) = [item for item in native["artifacts"] if item["name"] == name]
    record = read(relative(directory, "collection.json"))["provenance"]
    filename = original_path(record["artifact_path"], record["system"]).name
    reference = f"github-actions:{original['id']}/attempt-{original['run_attempt']}/{name}/{filename}"
    require(
        record["artifact_reference"] == reference
        and record["source_revision"] == original["head_sha"],
        "candidate package/source/native attempt reference differs",
    )
    return {
        "run_id": original["id"],
        "run_attempt": original["run_attempt"],
        "source_revision": original["head_sha"],
        "artifact_id": artifact["id"],
        "artifact_name": name,
        "artifact_reference": reference,
        "association": "original native metadata and retained build/installation attestation; archive/installed bytes re-admitted separately",
    }


@contextlib.contextmanager
def entry_root(root, entry, scratch, entry_provider):
    """Only one original ZIP is expanded at a time; original bytes stay intact."""
    with contextlib.ExitStack() as stack:
        archive_root = (
            stack.enter_context(entry_provider(entry, root)) if entry_provider else root
        )
        if "archive" not in entry:
            require(
                entry_provider is None,
                "streamed entry requires its original archive path",
            )
            yield root
            return
        archive_path = relative(Path(archive_root), entry["archive"])
        destination = scratch / "evidence" / entry["artifact_name"]
        destination.mkdir(parents=True)
        seen = set()
        with zipfile.ZipFile(archive_path) as archive:
            require(bool(archive.infolist()), "empty original Actions ZIP")
            for member in archive.infolist():
                name = PurePosixPath(member.filename)
                require(
                    not name.is_absolute()
                    and not PureWindowsPath(member.filename).is_absolute()
                    and ".." not in name.parts
                    and "\\" not in member.filename,
                    "unsafe original archive member",
                )
                require(
                    name.as_posix() not in seen, "duplicate original archive member"
                )
                seen.add(name.as_posix())
                require(
                    (member.external_attr >> 16) & 0o170000 != 0o120000,
                    "archive symlink refused",
                )
                target = destination / name
                if member.is_dir():
                    target.mkdir(parents=True, exist_ok=True)
                else:
                    target.parent.mkdir(parents=True, exist_ok=True)
                    with archive.open(member) as source, target.open("xb") as output:
                        import shutil

                        shutil.copyfileobj(source, output, 1_048_576)
        yield scratch / "evidence"


def audit(input_path, repository, budgets_path=None, entry_provider=None):
    root = input_path.resolve().parent
    errors, accepted = [], {}
    payload = read(input_path)
    require(payload["protocol"] == PROTOCOL, "unsupported audit input protocol")
    exact(
        payload,
        ("protocol", "repository", "source_revision", "run", "artifacts", "entries")
        + tuple(
            key
            for key in (
                "source_bundle",
                "jobs",
                "native_run",
                "native_jobs",
                "native_artifacts",
            )
            if key in payload
        ),
        "audit input",
    )
    nonempty(payload["repository"], "repository identity")
    require(
        re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", payload["repository"]),
        "invalid repository identity",
    )
    require(
        isinstance(payload["source_revision"], str)
        and re.fullmatch(r"[0-9a-f]{40}", payload["source_revision"]),
        "invalid exact source revision",
    )
    run = read(relative(root, payload["run"]))
    artifacts = read(relative(root, payload["artifacts"]))
    artifacts = artifacts["artifacts"] if isinstance(artifacts, dict) else artifacts
    validate_run(run, payload)
    jobs = read(relative(root, payload["jobs"])) if "jobs" in payload else []
    jobs = jobs["jobs"] if isinstance(jobs, dict) else jobs
    native = native_inputs(payload, root, run)
    budget_error = None
    try:
        budgets = (
            validate_budgets(read(budgets_path), payload["source_revision"])
            if budgets_path
            else None
        )
    except EVIDENCE_ERRORS as error:
        budgets, budget_error = None, str(error)
    with tempfile.TemporaryDirectory(prefix="yamaa-performance-replay-") as temporary:
        scratch = Path(temporary)
        if "source_bundle" in payload:
            repository = scratch / "source.git"
            subprocess.run(
                [
                    "git",
                    "clone",
                    "--bare",
                    str(relative(root, payload["source_bundle"])),
                    str(repository),
                ],
                check=True,
                capture_output=True,
            )
        seen = set()
        for number, entry in enumerate(payload["entries"]):
            try:
                exact(
                    entry,
                    (
                        "tuple",
                        "artifact_id",
                        "artifact_name",
                        "run_attempt",
                        "campaign",
                        "collections",
                    )
                    + (("archive",) if "archive" in entry else ()),
                    "entry",
                )
                lane = entry["tuple"]
                require(lane in tuples(), "unexpected performance tuple")
                key = json.dumps(lane, sort_keys=True)
                require(key not in seen, "duplicate performance tuple")
                seen.add(key)
                artifact, job = admit_artifact(entry, artifacts, run, jobs)
                parse_time = datetime.datetime.fromisoformat
                with tempfile.TemporaryDirectory(
                    dir=scratch, prefix=f"entry-{number}-"
                ) as entry_temporary:
                    entry_scratch = Path(entry_temporary)
                    with entry_root(
                        root, entry, entry_scratch, entry_provider
                    ) as evidence_root:
                        campaign = read(relative(evidence_root, entry["campaign"]))
                        require(
                            parse_time(job["started_at"])
                            <= parse_time(campaign["created_at"])
                            <= parse_time(artifact["created_at"]),
                            "campaign belongs to another job attempt",
                        )
                        require(
                            campaign["measurement_machine"]["runner_name"]
                            == job["runner_name"],
                            "campaign actual runner differs from producing job",
                        )
                        roles = [
                            collection["role"] for collection in entry["collections"]
                        ]
                        require(
                            bool(roles)
                            and len(roles) == len(set(roles))
                            and set(roles).issubset(ROLES)
                            and "candidate" in roles,
                            "duplicate/unexpected/absent collection role",
                        )
                        for collection in entry["collections"]:
                            exact(collection, ("role", "path"), "collection entry")
                            directory = relative(evidence_root, collection["path"])
                            require(
                                directory.parent.name == entry["artifact_name"],
                                "collection artifact association differs",
                            )
                            local = entry_scratch / collection["role"]
                            local.mkdir(parents=True)
                            association = (
                                native_collection(directory, lane, native)
                                if collection["role"] == "candidate"
                                else None
                            )
                            values = admit_collection(
                                directory,
                                lane,
                                collection["role"],
                                payload["source_revision"],
                                campaign,
                                repository,
                                local,
                            )
                            for case, result in values.items():
                                if association is not None:
                                    result["native_package_association"] = association
                                identity = (key, case, collection["role"])
                                require(
                                    identity not in accepted,
                                    "duplicate tuple/case combination",
                                )
                                accepted[identity] = result
            except EVIDENCE_ERRORS as error:
                errors.append({"entry": number, "reason": str(error)})
    cells = []
    for lane in tuples():
        key = json.dumps(lane, sort_keys=True)
        for case in case_names():
            cell = {"tuple": lane, "case": case}
            for role in ROLES:
                cell[role.replace("-", "_")] = accepted.get(
                    (key, case, role), {"status": "missing"}
                )
            cells.append(cell)
    passed = sum(cell["candidate"]["status"] == "pass" for cell in cells)
    failed = sum(cell["candidate"]["status"] == "fail" for cell in cells)
    correctness = (
        "fail"
        if errors or any(value["status"] == "fail" for value in accepted.values())
        else "pass"
        if accepted
        else "incomplete"
    )
    report = {
        "protocol": "public-performance-audit/1",
        "repository": payload["repository"],
        "source_revision": payload["source_revision"],
        "run_id": run["id"],
        "run_attempt": run["run_attempt"],
        "harness_correctness": {"status": correctness, "errors": errors},
        "native_source_admission": (
            {"status": native["status"], "reason": native.get("reason")}
            if native["status"] != "pass"
            else {
                "status": "pass",
                "run_id": native["run"]["id"],
                "run_attempt": native["run"]["run_attempt"],
            }
        ),
        "coverage": {
            "status": "pass" if passed == 567 and not errors else "incomplete",
            "required": 567,
            "passed": passed,
            "failed": failed,
            "missing": 567 - passed - failed,
        },
        "budgets": (
            {"status": "fail", "reason": budget_error, "results": []}
            if budget_error
            else evaluate_budgets(budgets, cells)
        ),
        "cells": cells,
        "baseline_gaps": "Callbacks have no retained compatible baseline. R runners retain Python baseline observations, whose cross-host ratios remain incomparable. Producer root-Build is a separate scope; full-graph Build, Save and child RSS have no comparable baseline.",
        "warm_rss_scope": child_peak_rss.SCOPE
        + "; one actual warm-child peak, never 30 invented per-repetition peaks",
    }
    report["accepted"] = all(
        report[key]["status"] == "pass"
        for key in ("harness_correctness", "coverage", "budgets")
    )
    return report


def markdown(report):
    return (
        f"# public performance replay\n\nSource: `{report['source_revision']}`; original run {report['run_id']}, attempt {report['run_attempt']}.\n\n"
        f"| Gate | Verdict |\n| --- | --- |\n| Harness correctness | {report['harness_correctness']['status']} |\n"
        f"| Coverage | {report['coverage']['status']} ({report['coverage']['passed']}/567) |\n"
        f"| Numeric budgets | {report['budgets']['status']} |\n\n{report['baseline_gaps']}\n\n"
        f"Warm RSS: {report['warm_rss_scope']}.\n\nAcceptance: {'pass' if report['accepted'] else 'incomplete or failed'}.\n"
    )


def main(argv=None, entry_provider=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument(
        "--repository", type=Path, default=Path(__file__).resolve().parents[2]
    )
    parser.add_argument("--budgets", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args(argv)
    args.output.mkdir(parents=True, exist_ok=False)
    try:
        report = audit(args.input, args.repository, args.budgets, entry_provider)
    except EVIDENCE_ERRORS as error:
        report = {
            "protocol": "public-performance-audit/1",
            "accepted": False,
            "harness_correctness": {"status": "fail", "errors": [str(error)]},
            "coverage": {"status": "incomplete", "required": 567, "passed": 0},
            "budgets": {"status": "not_evaluated"},
        }
    lifecycle.write_json(args.output / "performance-audit.json", report)
    text = (
        markdown(report)
        if "run_id" in report
        else f"# public performance replay\n\nRefused: {report['harness_correctness']['errors']}\n"
    )
    (args.output / "performance-audit.md").write_text(text, encoding="utf-8")
    return 1 if args.check and not report["accepted"] else 0


if __name__ == "__main__":
    raise SystemExit(main())
