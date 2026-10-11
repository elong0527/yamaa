"""Collect one approved scalable workload through both ordinary installed hosts.

Every cold child, warm-up, complete report and artifact is retained. No capacity
refusal becomes a sample, no failed sample is retried, and no release budget is
assigned. Run separately for candidate and a compatible previous release on the
same machine; missing baseline or phase/callback metrics remain unavailable.
"""

from __future__ import annotations

import argparse
import copy
import csv
import io
import json
import platform
import shutil
import statistics
from pathlib import Path

import measure_public_lifecycle as lifecycle
import prepare_public_workloads as workloads


def verify_workload(source, case_name, output):
    manifest = lifecycle.read_json(source / "manifest.json")
    (case,) = [row for row in manifest["cases"] if row["name"] == case_name]
    family, rows = case["family"], case["driving_rows"]
    if case_name != f"{family}-{rows}-{next(iter(case['dimensions'].values()))}":
        raise ValueError("workload name differs from its declaration")
    regenerated = output / "independent-replay"
    authored = workloads.prepare(regenerated, (rows,), (family,))
    (replayed,) = [row for row in authored["cases"] if row["name"] == case_name]
    if replayed != case:
        raise ValueError("declared workload differs from independent generator")
    for member in case["files"]:
        if (source / case_name / member["path"]).read_bytes() != (
            regenerated / case_name / member["path"]
        ).read_bytes():
            raise ValueError(
                "workload bytes differ from independent truth: " + member["path"]
            )
    if (source / "independent-generator.py").read_bytes() != Path(
        workloads.__file__
    ).read_bytes():
        raise ValueError("workload generator bytes differ from retained author")
    shutil.copyfile(source / "manifest.json", output / "workload-manifest.json")
    return case, regenerated / case_name


def validate(case, authored, child, request, response, record, repository):
    if response.get("failure") is not None:
        raise ValueError("worker failed: " + str(response["failure"]))
    lifecycle.validate_duration(response["import_duration"], request["host"])
    if len(response["repetitions"]) != len(request["repetitions"]):
        raise ValueError("missing measured repetition or warm-up")
    expected = lifecycle.read_json(authored / case["expected_output"])
    expected_columns = [
        ["phase", "str"],
        ["condition", "str"],
        ["requirement", "str"],
        ["spec_paths", "list[str]"],
        ["context", "str"],
    ]
    for actual, declaration in zip(response["repetitions"], request["repetitions"]):
        if (
            actual["declaration"] != declaration
            or actual["status"] != "success"
            or actual["evidence_capture_failures"]
        ):
            raise ValueError(
                "repetition failed or differs from declared input: "
                + str(actual.get("failure"))
            )
        lifecycle.actual_origins(actual["metadata"], record, repository)
        if (
            actual["native_call_order"] != ["public_build", "metadata_after_build"]
            or actual["unavailable"] != lifecycle.UNAVAILABLE
        ):
            raise ValueError("native call order or unavailable metrics differ")
        lifecycle.validate_rss(actual["rss"], request["host"], record["system"])
        for boundary in lifecycle.BOUNDARIES[1:]:
            lifecycle.validate_duration(actual["durations"][boundary], request["host"])
        views = lifecycle.read_json(child / actual["views"])
        for name in ("output", "output_after_save_2"):
            for key in ("columns", "row_count", "rows"):
                if views[name][key] != expected[key]:
                    raise ValueError(
                        "copied output differs from independent truth: "
                        + name
                        + "."
                        + key
                    )
        if views["issues"] != {"columns": expected_columns, "row_count": 0, "rows": []}:
            raise ValueError("public issues differ from independent empty truth")
        if views["verification_log"] is not None or views["warning_log"] is not None:
            raise ValueError("undeclared logs must remain null")
        if actual["save_returns"] != [True, True]:
            raise ValueError("held Saves failed")
        reports = {
            stage: lifecycle.read_json(child / actual["reports"][stage])
            for stage in ("build", "save_1", "save_2")
        }
        before = reports["build"]
        if (
            before.get("outcome") != "success"
            or before.get("diagnostics")
            or before.get("error")
            or before.get("unsupported")
        ):
            raise ValueError("complete Build report is unsuccessful")
        if actual["build_artifact_presence"] != {
            artifact["path"]: False for artifact in case["expected_artifacts"]
        }:
            raise ValueError("Build published a declared artifact before Save")
        nodes = before.get("nodes", [])
        if [Path(node["specification"]).name for node in nodes] != case[
            "expected_nodes"
        ]:
            raise ValueError("node ledger differs from independently declared graph")
        if any(
            node.get("outcome") != "success"
            or node.get("diagnostics")
            or node.get("unsupported")
            for node in nodes
        ):
            raise ValueError("node ledger contains a refused producer or root")
        reads = before.get("source_reads", [])
        if sorted(read["path"] for read in reads) != sorted(
            source["path"] for source in case["source_files"]
        ) or any(
            read.get("outcome") != "captured"
            or read.get("condition") is not None
            or read.get("snapshots_created") != 1
            for read in reads
        ):
            raise ValueError("source ledger differs from independently declared reads")
        if reports["save_1"] != reports["save_2"]:
            raise ValueError("repeated held Save changes complete observations")
        for stage in ("save_1", "save_2"):
            saved = copy.deepcopy(reports[stage])
            publication = saved.pop("publication", None)
            if publication is not None and publication.get("status") != "success":
                raise ValueError("complete Save report has no successful publication")
            artifacts = saved.pop("artifacts", None)
            if not artifacts:
                raise ValueError("complete Save report omits published artifacts")
            expected_artifacts = {
                Path(artifact["path"]).stem: artifact
                for artifact in case["expected_artifacts"]
            }
            if [artifact["name"] for artifact in artifacts] != list(expected_artifacts):
                raise ValueError("reported artifact identities/order differ from graph")
            for artifact in artifacts:
                expected_artifact = expected_artifacts[artifact["name"]]
                expected_content = (
                    authored / expected_artifact["expected"]
                ).read_bytes()
                content = artifact["content"].encode("utf-8")
                columns = next(
                    csv.reader(io.StringIO(expected_content.decode("utf-8")))
                )
                if (
                    content != expected_content
                    or artifact["byte_length"] != len(content)
                    or artifact["row_count"] != expected_artifact["row_count"]
                    or artifact["columns"] != columns
                    or artifact["profile"] != "csv"
                ):
                    raise ValueError("reported artifact differs from independent bytes")
            build = copy.deepcopy(before)
            build.pop("publication", None)
            build.pop("artifacts", None)
            if saved != build:
                raise ValueError("held Save changes completed Build observations")
            for artifact in case["expected_artifacts"]:
                name = actual["saved_artifacts"][stage][artifact["path"]]
                if (child / name).read_bytes() != (
                    authored / artifact["expected"]
                ).read_bytes():
                    raise ValueError(
                        "saved bytes differ from independently authored truth: "
                        + artifact["path"]
                    )
        # Some installed owners expose prospective artifact bytes before Save.
        # Retain that shape and compare them with the completed Save observations;
        # actual disk presence, not a report field, establishes zero publication.
        if (
            before.get("artifacts")
            and before["artifacts"] != reports["save_1"]["artifacts"]
        ):
            raise ValueError("prospective Build artifacts differ from completed Save")


def collect(args):
    if not args.compilation_clear:
        raise ValueError("explicit --compilation-clear attestation required")
    repository = args.repository.resolve()
    output = args.output.resolve()
    if any(
        output.is_relative_to(root) for root in lifecycle.checkout_roots(repository)
    ):
        raise ValueError("evidence must be outside all checkouts")
    output.mkdir(parents=True, exist_ok=False)
    plan = {
        "protocol": "scalable-public-lifecycle/1",
        "label": args.label,
        "purpose": args.purpose,
        "case": args.case,
        "accepted": False,
        "state": "preparing",
        "children": [],
        "unavailable": lifecycle.UNAVAILABLE,
        "numeric_budgets": None,
    }
    lifecycle.write_json(output / "collection.json", plan)
    try:
        case, authored = verify_workload(args.workloads.resolve(), args.case, output)
        records = {
            host: lifecycle.provenance(
                getattr(args, host + "_provenance"),
                repository,
                host,
                output / (host + "-installation"),
            )
            for host in ("python", "r")
        }
        if any(record["system"] != platform.system() for record in records.values()):
            raise ValueError("installed host system differs from measurement machine")
        plan["provenance"] = records
        plan["scope"] = {
            "cold": args.cold,
            "warm": args.warm,
            "warmups": args.warmups,
            "workload": case,
        }
        harness = output / "harness"
        harness.mkdir()
        for name in (
            Path(__file__).name,
            "measure_public_lifecycle.py",
            "prepare_public_workloads.py",
            "public_lifecycle_worker_protocol.json",
            *lifecycle.WORKERS.values(),
        ):
            shutil.copyfile(lifecycle.HERE / name, harness / name)
        samples = []
        for host in ("python", "r"):
            executable = lifecycle.worker_executable(
                getattr(args, host + "_executable"), repository
            )
            declarations = [
                ("cold", number, [{"index": number, "warmup": False}])
                for number in range(args.cold)
            ]
            declarations.append(
                (
                    "warm",
                    0,
                    [
                        {"index": number, "warmup": number < args.warmups}
                        for number in range(args.warmups + args.warm)
                    ],
                )
            )
            for mode, number, repetitions in declarations:
                name = f"{host}-{mode}-{number}"
                child = output / name
                child.mkdir()
                row = {
                    "child": name,
                    "host": host,
                    "mode": mode,
                    "status": "rejected",
                    "failure": None,
                }
                plan["children"].append(row)
                lifecycle.write_json(output / "collection.json", plan)
                try:
                    probe = lifecycle.compilation_probe()
                    lifecycle.write_json(child / "compilation-probe.json", probe)
                    if not probe["clear"]:
                        raise ValueError("compilation inventory is not clear")
                    declarations_for_child = []
                    for declaration in repetitions:
                        work = child / f"work-{declaration['index']}"
                        work.mkdir()
                        for member in case["files"]:
                            if member["path"].startswith("expected/"):
                                continue
                            target = work / member["path"]
                            target.parent.mkdir(parents=True, exist_ok=True)
                            shutil.copyfile(authored / member["path"], target)
                        environment = (
                            lifecycle.specification_identity(
                                work / case["environment"][host]
                            )
                            if case["environment"]
                            else None
                        )
                        declarations_for_child.append(
                            dict(
                                declaration,
                                work_directory=str(work),
                                entry=lifecycle.specification_identity(
                                    work / case["entry"]
                                ),
                                environment=environment,
                                output_name="out.csv",
                                artifact_names=[
                                    a["path"] for a in case["expected_artifacts"]
                                ],
                            )
                        )
                    request = {
                        "protocol": lifecycle.PROTOCOL,
                        "child": name,
                        "host": host,
                        "output": str(child),
                        "provenance": records[host],
                        "unavailable": lifecycle.UNAVAILABLE,
                        "repetitions": declarations_for_child,
                    }
                    lifecycle.write_json(child / "request.json", request)
                    command = lifecycle.worker_command(
                        host,
                        executable,
                        harness / lifecycle.WORKERS[host],
                        child / "request.json",
                    )
                    returncode = lifecycle.launch(command, child)
                    response = lifecycle.read_json(child / "response.json")
                    validate(
                        case,
                        authored,
                        child,
                        request,
                        response,
                        records[host],
                        repository,
                    )
                    if returncode["exit_code"]:
                        raise ValueError("worker returned a nonzero status")
                    row["status"] = "accepted"
                    for repetition in response["repetitions"]:
                        if repetition["declaration"]["warmup"]:
                            continue
                        for boundary, duration in repetition["durations"].items():
                            samples.append(
                                {
                                    "host": host,
                                    "mode": mode,
                                    "boundary": boundary,
                                    "duration_ns": lifecycle.validate_duration(
                                        duration, host
                                    ),
                                }
                            )
                except Exception as error:
                    row["failure"] = {
                        "class": type(error).__name__,
                        "message": str(error),
                    }
                lifecycle.write_json(output / "collection.json", plan)
        # The retained library association must also survive the entire interval.
        for host, record in records.items():
            if (
                Path(record["installed_origins"]["native"]).read_bytes()
                != (
                    output / (host + "-installation/installed-native-library.bin")
                ).read_bytes()
            ):
                raise ValueError("installed native library changed during collection")
        plan["accepted"] = all(
            child["status"] == "accepted" for child in plan["children"]
        )
        plan["state"] = "accepted" if plan["accepted"] else "rejected"
        if plan["accepted"] and args.purpose == "characterization":
            groups = {}
            for sample in samples:
                groups.setdefault(
                    (sample["host"], sample["mode"], sample["boundary"]), []
                ).append(sample["duration_ns"])
            plan["distribution"] = [
                {
                    "host": key[0],
                    "mode": key[1],
                    "boundary": key[2],
                    "count": len(values),
                    "min_ns": min(values),
                    "median_ns": statistics.median(values),
                    "max_ns": max(values),
                    "quartiles_ns": statistics.quantiles(
                        values, n=4, method="inclusive"
                    )
                    if len(values) > 1
                    else None,
                }
                for key, values in groups.items()
            ]
        lifecycle.write_json(output / "collection.json", plan)
        return plan
    except BaseException as error:
        plan.update(
            accepted=False,
            state="interrupted" if isinstance(error, KeyboardInterrupt) else "rejected",
            failure={"class": type(error).__name__, "message": str(error)},
        )
        lifecycle.write_json(output / "collection.json", plan)
        raise


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", type=Path, required=True)
    parser.add_argument("--workloads", type=Path, required=True)
    parser.add_argument("--case", required=True)
    parser.add_argument("--label", choices=("candidate", "baseline"), required=True)
    parser.add_argument(
        "--purpose", choices=("correctness-smoke", "characterization"), required=True
    )
    parser.add_argument("--output", type=Path, required=True)
    for host in ("python", "r"):
        parser.add_argument("--" + host + "-executable", type=Path, required=True)
        parser.add_argument("--" + host + "-provenance", type=Path, required=True)
    parser.add_argument("--cold", type=int, default=10)
    parser.add_argument("--warm", type=int, default=30)
    parser.add_argument("--warmups", type=int, default=3)
    parser.add_argument("--compilation-clear", action="store_true")
    args = parser.parse_args()
    if min(args.cold, args.warm, args.warmups) < 1:
        parser.error("cold/warm/warm-up counts must be positive")
    if (args.cold, args.warm, args.warmups) != (
        (1, 1, 1) if args.purpose == "correctness-smoke" else (10, 30, 3)
    ):
        parser.error(
            "use 1/1/1 for a correctness smoke or the approved 10/30/3 for characterization"
        )
    result = collect(args)
    print(
        json.dumps(
            {
                "output": str(args.output),
                "state": result["state"],
                "accepted": result["accepted"],
            }
        )
    )
    raise SystemExit(0 if result["accepted"] else 1)
