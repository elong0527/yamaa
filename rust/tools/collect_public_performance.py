"""Retain approved-size frontier or truth-admitted public characterization.

Use installed packages outside every checkout. A fresh truth child precedes all
timed children for a case. Refusals retain reached observations and child RSS,
and never become timing samples. Package/source association is a retained build
attestation; direct archive/native comparisons do not invent binary provenance.
"""

from __future__ import annotations

import argparse
import json
import platform
import shutil
import statistics
import subprocess
from pathlib import Path

import application_measurement_evidence as phases
import child_peak_rss
import measure_public_lifecycle as lifecycle
import measure_scalable_public as scalable
import prepare_public_workloads as workloads


def normalized_report(path, work):
    """Compare decoded reports across native and specification path spellings."""
    spellings = (str(work), lifecycle.specification_identity(work))

    def walk(value):
        if isinstance(value, str):
            for spelling in spellings:
                value = value.replace(spelling, "<work>")
            return value
        if isinstance(value, list):
            return [walk(item) for item in value]
        if isinstance(value, dict):
            return {key: walk(item) for key, item in value.items()}
        return value

    return walk(lifecycle.read_json(path))


def root_build_environment(case, authored):
    """Admit only generated producer environments with no active semantics."""
    if case["family"] != "producer":
        raise ValueError("the supported root-Build baseline is restricted to producers")
    path = authored / case["environment"]["python"]
    value = lifecycle.read_json(path)
    if (set(value) != {"schema_version", "language", "lock", "functions"}
            or value["schema_version"] != "1.0" or value["language"] != "python"
            or value["functions"] != {}):
        raise ValueError("omitted producer environment has active or unreviewed semantics")
    return {"path": str(path), "exact_declaration": value,
            "omission": "empty functions; only language/schema/packaging lock metadata; no codelist/study/submission fields",
            "graph_save_baseline": None,
            "memory_comparison_with_full_graph_lifecycle": None}


def validate_root_invocation(actual, request, record, repository):
    invocation = actual["build_invocation"]
    if (invocation["callable"] != "yamaa._reference_domain.yamaa_domain"
            or "environment" in invocation["parameters"]
            or "schema_root" not in invocation["parameters"]
            or invocation["schema_root"] != request["reference_schema"]
            or invocation["environment_passed"] is not False):
        raise ValueError("supported reference root-Build invocation changed")
    source = Path(invocation["source_file"])
    if not source.is_relative_to(Path(record["installed_origins"]["package"])):
        raise ValueError("reference callable source is outside its installed package")
    if source.read_bytes() != lifecycle.git_bytes(repository, record["source_revision"],
                                                 "python/src/yamaa/_reference_domain.py"):
        raise ValueError("reference callable source differs from the pinned previous release")
    if (actual["save_returns"] or actual["saved_files"] or actual.get("saved_artifacts")
            or any(name.startswith("held_save_") for name in actual["durations"])):
        raise ValueError("root-Build baseline entered or timed a Save")
    if any(actual["root_build_artifact_presence"].values()):
        raise ValueError("root-Build baseline published an artifact")


def validate_reference(case, authored, child, request, response, record, repository):
    """Compare actual legacy public results; unavailable ledgers stay unavailable."""
    if response.get("failure"):
        raise ValueError("reference worker failed: " + str(response["failure"]))
    if request["host"] != "python":
        raise ValueError("the Python reference baseline requires the Python host")
    if len(response["repetitions"]) != len(request["repetitions"]):
        raise ValueError("missing reference repetition")
    lifecycle.validate_duration(response["import_duration"], "python")
    expected = lifecycle.read_json(authored / case["expected_output"])
    for actual, declaration in zip(response["repetitions"], request["repetitions"]):
        if actual["status"] != "success" or actual["declaration"] != declaration or actual["evidence_capture_failures"]:
            raise ValueError("reference repetition failed: " + str(actual.get("failure")))
        lifecycle.actual_origins(actual["metadata"], record, repository)
        root_only = request.get("reference_root_build", False)
        boundaries = [name for name in lifecycle.BOUNDARIES[1:]
                      if not root_only or not name.startswith("held_save_")]
        for boundary in boundaries:
            lifecycle.validate_duration(actual["durations"][boundary], "python")
        views = lifecycle.read_json(child / actual["views"])
        for name in ("output", "output_after_build_repeat" if root_only else "output_after_save_2"):
            for key in ("columns", "row_count", "rows"):
                if views[name][key] != expected[key]:
                    raise ValueError("reference differs from independent truth: " + name + "." + key)
        if views["issues"]["row_count"] or views["verification_log"] is not None or views["warning_log"] is not None:
            raise ValueError("reference issues/logs differ from independent truth")
        if any(actual["build_artifact_presence"].values()):
            raise ValueError("reference published before explicit Save; lifecycle baseline incompatible")
        if root_only:
            root_build_environment(case, authored)
            validate_root_invocation(actual, request, record, repository)
            continue
        for stage in ("save_1", "save_2"):
            for artifact in case["expected_artifacts"]:
                name = actual.get("saved_artifacts", {}).get(stage, {}).get(artifact["path"])
                if name is None or lifecycle.retained_bytes(child / name) != (authored / artifact["expected"]).read_bytes():
                    raise ValueError("reference Save differs from independent bytes: " + artifact["path"])


def collect(args):
    repository = args.repository.resolve()
    output = args.output.resolve()
    if any(output.is_relative_to(root) for root in lifecycle.checkout_roots(repository)):
        raise ValueError("evidence must be outside all checkouts")
    output.mkdir(parents=True, exist_ok=False)
    manifest = workloads.prepare(output / "workloads")
    harness = output / "harness"
    harness.mkdir()
    for name in (Path(__file__).name, "child_peak_rss.py", "measure_public_lifecycle.py",
                 "measure_scalable_public.py", "prepare_public_workloads.py",
                 "public_lifecycle_worker_protocol.json", "application_measurement_evidence.py", *lifecycle.WORKERS.values()):
        shutil.copyfile(lifecycle.HERE / name, harness / name)
    record = lifecycle.read_json(args.provenance)
    phase_measurements = bool(getattr(args, "phase_measurements", False))
    if phase_measurements and args.evaluator != "public":
        raise ValueError("phase measurements require the instrumented public candidate")
    root_only = args.scope == "producer-root-build"
    if root_only and (args.host != "python" or args.evaluator != "python-reference"):
        raise ValueError("producer root-Build scope needs the installed Python reference")
    plan = {
        "protocol": "public-performance/1", "purpose": args.purpose,
        "host": args.host, "source_revision": record["source_revision"],
        "harness_checkout_head": lifecycle.surrounding_git_head(lifecycle.HERE),
        "truth_repository_head": lifecycle.surrounding_git_head(repository),
        "provenance": record, "accepted": False, "cases": [],
        "sampling": {"cold": 10, "warm": 30, "warmups": 3},
        "numeric_budgets": None, "phase_durations": None,
        "phase_measurements_required": phase_measurements,
        "actual_callback_counts": None,
        "evaluator": args.evaluator, "selected_cases": args.cases,
        "selected_families": args.families,
        "measurement_scope": args.scope,
        "graph_save_baseline": None if root_only else "complete declared artifacts",
        "memory_comparison_with_full_graph_lifecycle": None if root_only else "same declared lifecycle required",
    }
    lifecycle.write_json(output / "collection.json", plan)
    # Frontier admission retains origin/archive observations but does not admit
    # R clocks or make performance claims. Characterization uses full provenance.
    if args.purpose == "characterization":
        record = lifecycle.provenance(args.provenance, repository, args.host,
                                      output / "installation")
    else:
        retained = output / "installation"
        retained.mkdir()
        shutil.copyfile(args.provenance, retained / "declared.json")
        for name in ("artifact_path", "installation_evidence"):
            source = Path(record[name])
            shutil.copyfile(source, retained / source.name)
        record["installed_origins"] = {
            key: lifecycle.outside_checkout(value, repository)
            for key, value in record["installed_origins"].items()
        }
        if args.host == "r":
            for spec in record["collectors"].values():
                spec["origin"] = lifecycle.outside_checkout(spec["origin"], repository)
            record["clock_evidence"]["installed_library"] = lifecycle.outside_checkout(
                record["clock_evidence"]["installed_library"], repository)
        shutil.copyfile(record["installed_origins"]["native"], retained / "installed-native-library.bin")
        lifecycle.write_json(retained / "artifact-byte-comparisons.json",
                             lifecycle.artifact_bytes(record, repository))
    reference_schema = None
    if args.evaluator == "python-reference":
        reference_schema = output / "reference-schema"
        reference_schema.mkdir()
        for path in lifecycle.git_paths(repository, record["source_revision"], "yaml"):
            target = reference_schema / Path(path).relative_to("yaml")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(lifecycle.git_bytes(repository, record["source_revision"], path))
    executable = lifecycle.worker_executable(args.executable, repository)
    for case in manifest["cases"]:
        if args.families and case["family"] not in args.families:
            continue
        if root_only and case["family"] != "producer":
            continue
        if args.cases and case["name"] not in args.cases:
            continue
        row = {"case": case["name"], "truth_accepted": False, "accepted": False,
               "children": [], "distribution": None}
        if root_only:
            row["supported_reference_invocation"] = root_build_environment(
                case, output / "workloads" / case["name"])
        plan["cases"].append(row)
        lifecycle.write_json(output / "collection.json", plan)
        declarations = [("truth", 0, [{"index": 0, "warmup": False}])]
        if case["family"] == "callback" and args.evaluator == "public":
            declarations.append(("callback-observer", 0, [{"index": 0, "warmup": False}]))
        if args.purpose == "characterization":
            declarations += [("cold", n, [{"index": n, "warmup": False}]) for n in range(10)]
            declarations += [("warm", 0, [{"index": n, "warmup": n < 3} for n in range(33)])]
        samples = []
        for mode, number, repetitions in declarations:
            child = output / (case["name"] + f"-{mode}-{number}")
            child.mkdir()
            observation = {"child": child.name, "mode": mode, "accepted": False,
                           "failure": None, "peak_rss": None,
                           "counts_toward_timing": False}
            row["children"].append(observation)
            try:
                if mode != "truth" and not row["truth_accepted"]:
                    raise ValueError("not launched: independent-truth preflight refused")
                probe = lifecycle.compilation_probe()
                lifecycle.write_json(child / "compilation-probe.json", probe)
                if args.purpose == "characterization" and not probe["clear"]:
                    raise ValueError("compilation is active or cannot be ruled out")
                staged = []
                for declaration in repetitions:
                    work = child / f"work-{declaration['index']}"
                    shutil.copytree(output / "workloads" / case["name"], work,
                                    ignore=shutil.ignore_patterns("expected"))
                    staged.append(dict(declaration, work_directory=str(work),
                        entry=lifecycle.specification_identity(work / case["entry"]),
                        environment=(lifecycle.specification_identity(work / case["environment"][args.host])
                                     if case["environment"] else None),
                        output_name="out.csv",
                        artifact_names=[a["path"] for a in case["expected_artifacts"]]))
                request = {"protocol": lifecycle.PROTOCOL, "child": child.name,
                           "host": args.host, "output": str(child), "provenance": record,
                           "unavailable": lifecycle.unavailable(phase_measurements), "repetitions": staged,
                           "phase_measurements": phase_measurements,
                           "evaluator": args.evaluator, "reference_schema": str(reference_schema) if reference_schema else None,
                           "reference_root_build": root_only,
                           "observe_callbacks": mode == "callback-observer",
                           "compress_evidence": args.purpose == "characterization"}
                lifecycle.write_json(child / "request.json", request)
                process = lifecycle.launch(lifecycle.worker_command(args.host, executable,
                    harness / lifecycle.WORKERS[args.host], child / "request.json"), child)
                observation["peak_rss"] = process["peak_rss"]
                response = lifecycle.read_json(child / "response.json")
                if response.get("pid") != process["pid"]:
                    observation["unqualified_launcher_peak_rss"] = observation["peak_rss"]
                    observation["peak_rss"] = None
                    raise ValueError("worker PID differs from measured process; child RSS unavailable")
                # Preserve failures before admission. Reached reports are already
                # retained by the worker, even when a view/Save subsequently fails.
                validator = validate_reference if args.evaluator == "python-reference" else scalable.validate
                validator(case, output / "workloads" / case["name"], child,
                          request, response, record, repository)
                if process["exit_code"]:
                    raise ValueError("worker returned a nonzero status")
                child_peak_rss.validate(process["peak_rss"], record["system"])
                observation["accepted"] = True
                if phase_measurements:
                    observation["actual_application_measurements"] = [phases.validate_repetition(case, child, rep, lifecycle.read_json) for rep in response["repetitions"]]
                if mode == "truth":
                    row["truth_accepted"] = True
                elif mode == "callback-observer":
                    actual = response["repetitions"][0]["callback_observation"]
                    expected_output = lifecycle.read_json(output / "workloads" / case["name"] / case["expected_output"])
                    expected_values = [workloads.typed(7.25, "float"), workloads.typed(0.0, "float")]
                    expected_values += [cells[1] for cells in expected_output["rows"] if cells[1]["type"] != "missing"]
                    expected_trace = [{"parameters": {"x": value}, "result": value, "failure": None}
                                      for value in expected_values]
                    if actual["count"] != len(expected_trace) or actual["trace"] != expected_trace:
                        raise ValueError("actual delegated callback trace differs from independent truth")
                    ordinary_child = output / row["children"][0]["child"]
                    ordinary = lifecycle.read_json(ordinary_child / "response.json")["repetitions"][0]
                    for stage in ("build", "save_1", "save_2"):
                        lhs = normalized_report(ordinary_child / ordinary["reports"][stage], Path(ordinary["declaration"]["work_directory"]))
                        rhs = normalized_report(child / response["repetitions"][0]["reports"][stage], Path(staged[0]["work_directory"]))
                        if lhs != rhs:
                            raise ValueError("callback observer changed complete public observations")
                    observation["actual_callback_calls"] = actual["count"]
                    observation["accepted"] = True
                else:
                    observation["counts_toward_timing"] = True
                    if mode == "cold":
                        samples.append({"mode": mode, "boundary": "package_import",
                            "duration_ns": lifecycle.validate_duration(response["import_duration"], args.host)})
                    for repetition in response["repetitions"]:
                        if not repetition["declaration"]["warmup"]:
                            for boundary, duration in repetition["durations"].items():
                                samples.append({"mode": mode, "boundary": boundary,
                                    "duration_ns": lifecycle.validate_duration(duration, args.host)})
            except Exception as error:
                observation["accepted"] = False
                observation["counts_toward_timing"] = False
                observation["failure"] = {"class": type(error).__name__, "message": str(error)}
            lifecycle.write_json(output / "collection.json", plan)
        row["accepted"] = all(child["accepted"] for child in row["children"])
        if row["accepted"] and args.purpose == "characterization":
            groups = {}
            for sample in samples:
                groups.setdefault((sample["mode"], sample["boundary"]), []).append(sample["duration_ns"])
            row["distribution"] = [{"mode": mode, "boundary": boundary,
                "count": len(values), "raw_ns": values, "median_ns": statistics.median(values),
                "min_ns": min(values), "max_ns": max(values),
                "quartiles_ns": statistics.quantiles(values, n=4, method="inclusive")}
                for (mode, boundary), values in groups.items()]
        lifecycle.write_json(output / "collection.json", plan)
    if phase_measurements:
        plan["phase_durations"] = [{"case":case["case"],"child":child["child"],"repetitions":child["actual_application_measurements"]} for case in plan["cases"] for child in case["children"] if child["accepted"]]
    if phase_measurements:
        plan["actual_callback_counts"] = [{"case":case["case"],"child":child["child"],"repetitions":[rep["function_calls"] for rep in child["actual_application_measurements"]]} for case in plan["cases"] for child in case["children"] if child["accepted"]]
    else:
        plan["actual_callback_counts"] = [
            {"case": case["case"], "combined_activation_and_derivation_calls": child["actual_callback_calls"],
             "stage_counts": None}
            for case in plan["cases"] for child in case["children"]
            if child["accepted"] and "actual_callback_calls" in child
        ] or None
    plan["accepted"] = all(case["accepted"] for case in plan["cases"]) and (
        not root_only or bool(plan["cases"]))
    lifecycle.write_json(output / "collection.json", plan)
    return plan


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--phase-measurements", action="store_true")
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--cases", nargs="+")
    parser.add_argument("--families", nargs="+", choices=workloads.FAMILIES)
    parser.add_argument("--scope", choices=("full-lifecycle", "producer-root-build"), default="full-lifecycle")
    parser.add_argument("--evaluator", choices=("public", "python-reference"), default="public")
    parser.add_argument("--host", choices=("python", "r"), required=True)
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--provenance", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--purpose", choices=("capacity-frontier", "characterization"), required=True)
    result = collect(parser.parse_args())
    print(json.dumps({"accepted": result["accepted"], "cases": len(result["cases"])}))
    raise SystemExit(0 if result["accepted"] else 1)
