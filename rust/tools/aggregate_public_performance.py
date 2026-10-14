"""Aggregate genuine reviewed reader artifacts; arbitrary fragments cannot qualify.

The original successful collection and reader attempt are different identities.
Collection-only aggregation may run after successful unit dependencies, while
numeric acceptance also requires separately retained original reader completion.
No engine, raw capture substitution, RSS pooling or content digest is used.
"""

from __future__ import annotations

import argparse
import functools
import json
import math
import re
import tempfile
import uuid
from pathlib import Path

import audit_public_performance as auditor

PROTOCOL = "public-performance-audit-aggregate-input/1"
require, exact, read, relative = (
    auditor.require,
    auditor.exact,
    auditor.read,
    auditor.relative,
)


@functools.cache
def metric_inventory():
    return read(Path(auditor.__file__).with_suffix(".schema.json"))["$defs"]["rule"][
        "properties"
    ]["metric"]["enum"]


def validate_reader_run(run, repository, revision):
    require(
        type(run["id"]) is int
        and run["id"] > 0
        and type(run["run_attempt"]) is int
        and run["run_attempt"] > 0
        and run["repository"]["full_name"] == repository
        and run["head_repository"]["full_name"] == repository
        and run["head_sha"] == revision
        and run["path"] == auditor.READER_WORKFLOW
        and (
            (run["event"] == "workflow_run" and run["head_branch"] == "main")
            or (
                run["event"] == "push"
                and run["head_branch"] == "evidence/issue-1998-replay"
            )
        ),
        "untrusted reader repository/reviewed source/workflow/run/attempt",
    )
    auditor.nonempty(run["head_branch"], "original reader source branch")
    require(
        (run["status"] == "completed" and run["conclusion"] == "success")
        or (run["status"] == "in_progress" and run["conclusion"] is None),
        "failed/cancelled/incomplete original reader attempt",
    )


def reader_completion(payload, root, run, repository, revision):
    completion = (
        read(relative(root, payload["reader_completion_run"]))
        if "reader_completion_run" in payload
        else run
    )
    if "reader_completion_run" in payload:
        validate_reader_run(completion, repository, revision)
        require(
            all(
                completion[key] == run[key]
                for key in (
                    "id",
                    "run_attempt",
                    "head_sha",
                    "head_branch",
                    "path",
                    "event",
                    "repository",
                    "head_repository",
                )
            )
            and all(
                key not in run or completion.get(key) == run[key]
                for key in ("created_at", "run_number", "workflow_id", "run_started_at")
            ),
            "completion receipt belongs to another original reader attempt",
        )
        require(
            completion["status"] == "completed"
            and completion["conclusion"] == "success",
            "reader completion receipt is not terminal success",
        )
    return {
        "status": "pass" if completion["status"] == "completed" else "incomplete",
        "evidence": "completion-receipt"
        if "reader_completion_run" in payload
        else "original-snapshot",
        "run_id": run["id"],
        "run_attempt": run["run_attempt"],
    }


def admit_fragment_artifact(entry, run, jobs, artifacts, unit):
    exact(
        entry,
        ("selector", "artifact_id", "artifact_name")
        + (("archive",) if "archive" in entry else ()),
        "fragment entry",
    )
    require(
        entry["selector"] == unit["selector"],
        "fragment selector differs from original catalog",
    )
    require(
        type(entry["artifact_id"]) is int and entry["artifact_id"] > 0,
        "invalid fragment artifact ID",
    )
    name = "performance-unit-" + unit["key"]
    matches = [
        artifact for artifact in artifacts if artifact["id"] == entry["artifact_id"]
    ]
    require(len(matches) == 1, "missing/duplicate original reader artifact ID")
    artifact = matches[0]
    require(
        artifact["name"] == entry["artifact_name"] == name
        and type(artifact["id"]) is int
        and artifact["id"] > 0
        and artifact["expired"] is False
        and type(artifact["size_in_bytes"]) is int
        and artifact["size_in_bytes"] > 0
        and type(artifact["workflow_run"]["id"]) is int
        and artifact["workflow_run"]["id"] == run["id"]
        and artifact["workflow_run"]["head_sha"] == run["head_sha"],
        "reader artifact availability/source/run differs",
    )
    expected_job = "performance-replay-unit (" + unit["key"] + ")"
    matches = [job for job in jobs if job["name"] == expected_job]
    require(len(matches) == 1, "missing/duplicate actual reader producing job")
    job = matches[0]
    require(
        type(job["id"]) is int
        and job["id"] > 0
        and job["run_id"] == run["id"]
        and type(job["run_id"]) is int
        and job["run_attempt"] == run["run_attempt"]
        and type(job["run_attempt"]) is int
        and job["head_sha"] == run["head_sha"]
        and job["status"] == "completed"
        and job["conclusion"] == "success",
        "actual reader job failed/incomplete or source/attempt differs",
    )
    timestamp = auditor.native_admission.timestamp
    # The unmodified API snapshot can retain old attempts with the same name.
    # Only the exact selected successful job's interval establishes ambiguity.
    require(
        sum(
            item["name"] == name
            and timestamp(job["started_at"])
            <= timestamp(item["created_at"])
            <= timestamp(job["completed_at"])
            for item in artifacts
        )
        == 1,
        "ambiguous reader artifact in selected producing attempt",
    )
    require(
        timestamp(job["started_at"])
        <= timestamp(artifact["created_at"])
        <= timestamp(job["completed_at"]),
        "reader artifact outside actual producing job interval",
    )
    return artifact, job


def validate_vectors(value, role, case):
    metrics = value["metrics"]
    require(
        isinstance(metrics, dict) and metrics, "absent validated raw metric vectors"
    )
    known = metric_inventory()
    require(set(metrics).issubset(known), "unexpected fragment metric scope")
    expected = {
        f"{mode}.{boundary}.median_ns"
        for mode in ("cold", "warm")
        for boundary in auditor.lifecycle.BOUNDARIES[1:]
        if role != "producer-root-build-baseline"
        or not boundary.startswith("held_save_")
    }
    expected |= {
        "cold.package_import.median_ns",
        "cold.child_peak_rss.median_bytes",
        "warm.child_peak_rss.median_bytes",
    }
    require(expected.issubset(metrics), "missing original lifecycle/RSS vector")
    unavailable = value["unavailable"]
    require(
        isinstance(unavailable, list) and len(unavailable) == len(set(unavailable)),
        "invalid fragment unavailable scopes",
    )
    phased = role == "candidate" and "phase_durations" not in unavailable
    require(
        unavailable
        == (list(auditor.lifecycle.unavailable(phased)) if role == "candidate" else []),
        "fragment unavailable scopes differ",
    )
    if phased:
        phases = (
            "preparation",
            "build_service",
            "input",
            "execution_admission",
            "execution_derivation",
            "output_keys",
            "verification",
            "output_preparation",
            "held_save_1",
            "held_save_2",
        )
        expected |= {
            f"{mode}.phase.{phase}.median_ns"
            for mode in ("cold", "warm")
            for phase in phases
        }
        require(expected.issubset(metrics), "missing original owner-local phase vector")
        activation = {f"{mode}.phase.activation.median_ns" for mode in ("cold", "warm")}
        require(
            not (set(metrics) & activation) or activation.issubset(metrics),
            "partial actual activation vectors",
        )
        expected |= set(metrics) & activation
    require(set(metrics) == expected, "unsupported fragment metric inventory")
    for name, vector in metrics.items():
        count = (
            1
            if name == "warm.child_peak_rss.median_bytes"
            else 10
            if name.startswith("cold.")
            else 30
        )
        require(
            type(vector["count"]) is int
            and vector["count"] == count
            and isinstance(vector["raw"], list)
            and len(vector["raw"]) == count,
            "raw vector repetition/child count differs",
        )
        unit = "bytes" if name.endswith("median_bytes") else "nanoseconds"
        require(
            all(
                type(v) in (int, float) and math.isfinite(v) and v > 0
                for v in vector["raw"]
            ),
            "invalid raw fragment clock/RSS value",
        )
        recomputed = (
            auditor.stats(vector["raw"], unit)
            if count > 1
            else {
                "count": 1,
                "raw": vector["raw"],
                "unit": unit,
                "median": vector["raw"][0],
                "min": vector["raw"][0],
                "max": vector["raw"][0],
                "quartiles": None,
            }
        )
        require(
            vector == recomputed,
            "fragment statistics differ from original validated raw vector",
        )
    calls = value["actual_function_calls"]
    require(isinstance(calls, list), "absent actual function ledger")
    expected_calls = (
        [
            (
                f"{case}-{mode}-{number}",
                mode,
                declaration["index"],
                declaration["warmup"],
            )
            for mode, number, declarations in auditor.schedule(
                {"family": case.split("-")[0]}, role
            )
            if mode in ("cold", "warm")
            for declaration in declarations
        ]
        if phased
        else []
    )
    require(
        len(calls) == len(expected_calls), "missing original warmup/function records"
    )
    for observation, schedule in zip(calls, expected_calls):
        exact(
            observation,
            ("child", "mode", "index", "warmup", "calls"),
            "actual function observation",
        )
        require(
            type(observation["index"]) is int
            and type(observation["warmup"]) is bool
            and (
                observation["child"],
                observation["mode"],
                observation["index"],
                observation["warmup"],
            )
            == schedule,
            "actual function repetition/warmup identity differs",
        )
        require(
            isinstance(observation["calls"], list) and observation["calls"],
            "absent actual function counts",
        )
        seen = set()
        for call in observation["calls"]:
            exact(
                call,
                ("node", "stage", "entered", "succeeded", "failed"),
                "actual function count",
            )
            node = call["node"]
            require(
                node is None or (type(node) is int and 0 <= node < 64),
                "invalid function node",
            )
            require(
                call["stage"]
                in (
                    "activation",
                    "execution_admission",
                    "execution_derivation",
                    "output_keys",
                    "verification",
                    "execution_finished",
                ),
                "invalid function stage",
            )
            identity = (node, call["stage"])
            require(identity not in seen, "duplicate actual function count")
            seen.add(identity)
            require(
                all(
                    type(call[k]) is int and 0 <= call[k] < 2**64
                    for k in ("entered", "succeeded", "failed")
                )
                and call["failed"] == 0
                and call["entered"] == call["succeeded"],
                "failed/incomplete actual function returns",
            )
    callbacks = value["actual_callback_counts"]
    require(isinstance(callbacks, list), "absent actual callback counts")
    callback = role == "candidate" and case.startswith("callback-")
    require(
        (len(callbacks) == 1 and type(callbacks[0]) is int and callbacks[0] > 0)
        if callback
        else callbacks == [],
        "actual observer callback scope/count differs",
    )


def validate_case_associations(value, role, case, unit, catalog, native):
    fields = (
        "status",
        "metrics",
        "unavailable",
        "actual_callback_counts",
        "actual_function_calls",
        "measurement_host",
        "source_revision",
        "performance_collection_association",
    ) + (("native_package_association",) if role == "candidate" else ())
    exact(value, fields, "qualified fragment case")
    lane = unit["selector"]["tuple"]
    require(
        value["status"] == "pass"
        and value["measurement_host"]
        == (lane["host"] if role == "candidate" else "python"),
        "failed/incompatible fragment case",
    )
    require(
        isinstance(value["source_revision"], str)
        and re.fullmatch(r"[0-9a-f]{40}", value["source_revision"]),
        "invalid original case source",
    )
    if role == "candidate":
        require(
            value["source_revision"] == catalog["original"]["source_revision"],
            "measured source relabeled to reader source",
        )
        association = value["native_package_association"]
        exact(
            association,
            (
                "run_id",
                "run_attempt",
                "source_revision",
                "artifact_id",
                "artifact_name",
                "artifact_reference",
                "association",
            ),
            "original native package association",
        )
        name = (
            f"native-python-{lane['os']}-3.14"
            if lane["host"] == "python"
            else f"native-r-{lane['os']}"
        )
        (artifact,) = [
            item for item in native["selected_artifacts"] if item["name"] == name
        ]
        require(
            all(
                association[key] == expected
                for key, expected in {
                    "run_id": native["run"]["id"],
                    "run_attempt": native["run"]["run_attempt"],
                    "source_revision": native["run"]["head_sha"],
                    "artifact_id": artifact["id"],
                    "artifact_name": name,
                }.items()
            ),
            "native package source/run/attempt/artifact differs",
        )
        prefix = f"github-actions:{native['run']['id']}/attempt-{native['run']['run_attempt']}/{name}/"
        reference = association["artifact_reference"]
        require(
            isinstance(reference, str)
            and reference.startswith(prefix)
            and bool(reference[len(prefix) :])
            and not any(c in reference[len(prefix) :] for c in ("/", "\\", "\x00")),
            "original native artifact reference differs",
        )
        require(
            association["association"]
            == "original native metadata and retained build/installation attestation; archive/installed bytes re-admitted separately",
            "unsupported native association scope",
        )
    association = value["performance_collection_association"]
    expected = {
        "artifact_id": unit["artifact"]["id"],
        "artifact_name": unit["artifact"]["name"],
        "run_id": catalog["original"]["run_id"],
        "run_attempt": catalog["original"]["run_attempt"],
        "producing_job": unit["job"]["name"],
        "producing_job_id": unit["job"]["id"],
        "runner_name": unit["job"]["runner_name"],
        "variant": unit["selector"]["variant"],
    }
    exact(association, (*expected, "campaign_id"), "original collection association")
    require(
        all(association[key] == value for key, value in expected.items()),
        "original performance artifact/job/runner association differs",
    )
    require(
        str(uuid.UUID(association["campaign_id"])) == association["campaign_id"],
        "invalid original campaign identity",
    )
    validate_vectors(value, role, case)


def admit_fragment(fragment, unit, catalog, run, native):
    exact(
        fragment,
        (
            "protocol",
            "original",
            "unit",
            "reader",
            "sampling",
            "raw_validation_scope",
            "harness_correctness",
            "native_source_admission",
            "run_completion",
            "unit_coverage",
            "unit_gate",
            "cells",
            "measurement_scopes",
            "budgets",
            "accepted",
        ),
        "reader fragment",
    )
    require(
        fragment["protocol"] == "public-performance-audit-fragment/1"
        and fragment["accepted"] is False
        and fragment["budgets"] == {"status": "not_evaluated"},
        "reader fragment cannot supply budget acceptance",
    )
    require(
        fragment["original"] == catalog["original"] and fragment["unit"] == unit,
        "fragment original manifest/catalog association differs",
    )
    require(
        fragment["reader"]
        == {
            "revision": run["head_sha"],
            "repository": catalog["original"]["repository"],
            "run_id": run["id"],
            "run_attempt": run["run_attempt"],
        },
        "fragment actual reader source/run/attempt differs",
    )
    require(
        fragment["sampling"] == auditor.SAMPLING
        and fragment["raw_validation_scope"] == auditor.RAW_VALIDATION_SCOPE
        and fragment["measurement_scopes"] == auditor.measurement_scopes(),
        "fragment raw-validation/sampling/scope differs",
    )
    required = sum(len(cases) for cases in unit["cases_by_role"].values())
    require(
        fragment["unit_gate"] == {"status": "pass", "key": unit["key"]}
        and fragment["unit_coverage"]
        == {"required": required, "passed": required, "missing_or_failed": 0},
        "incomplete or failed independent unit",
    )
    require(
        fragment["harness_correctness"] == {"status": "pass", "errors": []}
        and fragment["native_source_admission"]
        == {
            "status": "pass",
            "run_id": catalog["original"]["native_run_id"],
            "run_attempt": catalog["original"]["native_run_attempt"],
        }
        and fragment["run_completion"]["status"] == "pass"
        and fragment["run_completion"]["run_id"] == catalog["original"]["run_id"]
        and fragment["run_completion"]["run_attempt"]
        == catalog["original"]["run_attempt"],
        "unit original correctness/native/completion qualification differs",
    )
    values, seen, campaigns = {}, set(), set()
    for cell in fragment["cells"]:
        exact(
            cell,
            ("tuple", "case", *(role.replace("-", "_") for role in auditor.ROLES)),
            "fragment cell",
        )
        case = cell["case"]
        require(
            cell["tuple"] == unit["selector"]["tuple"]
            and case in unit["cases_by_role"]["candidate"]
            and case not in seen,
            "duplicate/extra/moved original tuple-case cell",
        )
        seen.add(case)
        for role in auditor.ROLES:
            value = cell[role.replace("-", "_")]
            if case not in unit["cases_by_role"].get(role, []):
                require(value == {"status": "missing"}, "unexpected role-case scope")
                continue
            validate_case_associations(value, role, case, unit, catalog, native)
            campaigns.add(value["performance_collection_association"]["campaign_id"])
            values[(json.dumps(cell["tuple"], sort_keys=True), case, role)] = value
    require(
        seen == set(unit["cases_by_role"]["candidate"])
        and len(values) == required
        and len(campaigns) == 1,
        "missing original cases/roles or mixed actual campaigns",
    )
    return values


def aggregate(input_path, reviewed_revision, budgets_path=None, entry_provider=None):
    require(
        isinstance(reviewed_revision, str)
        and re.fullmatch(r"[0-9a-f]{40}", reviewed_revision),
        "trusted reviewed reader revision must be explicit",
    )
    root, payload = input_path.resolve().parent, read(input_path)
    exact(
        payload,
        (
            "protocol",
            "original_manifest",
            "reader_run",
            "reader_jobs",
            "reader_artifacts",
            "fragments",
        )
        + (("reader_completion_run",) if "reader_completion_run" in payload else ()),
        "aggregate input",
    )
    require(payload["protocol"] == PROTOCOL, "unsupported aggregate input protocol")
    original_path = relative(root, payload["original_manifest"])
    catalog = auditor.replay_catalog(original_path)
    _, original, source_run, _, _, native, source_completion = auditor.input_context(
        original_path
    )
    run = read(relative(root, payload["reader_run"]))
    jobs, artifacts = [
        read(relative(root, payload[key]))
        for key in ("reader_jobs", "reader_artifacts")
    ]
    jobs = jobs["jobs"] if isinstance(jobs, dict) else jobs
    artifacts = artifacts["artifacts"] if isinstance(artifacts, dict) else artifacts
    validate_reader_run(run, original["repository"], reviewed_revision)
    completion = reader_completion(
        payload, root, run, original["repository"], reviewed_revision
    )
    units = {unit["key"]: unit for unit in catalog["units"]}
    require(isinstance(payload["fragments"], list), "fragment inventory must be a list")
    values, errors, seen, admitted = {}, [], set(), []
    for number, entry in enumerate(payload["fragments"]):
        try:
            key = auditor.unit_key(entry["selector"])
            require(
                key in units and key not in seen, "duplicate/unexpected reader unit"
            )
            seen.add(key)
            unit = units[key]
            artifact, job = admit_fragment_artifact(entry, run, jobs, artifacts, unit)
            with (
                tempfile.TemporaryDirectory(
                    prefix="yamaa-performance-fragment-"
                ) as temporary,
                auditor.entry_root(
                    root,
                    entry,
                    Path(temporary),
                    entry_provider,
                    artifact["size_in_bytes"],
                ) as evidence,
            ):
                fragment = read(
                    relative(
                        evidence,
                        entry["artifact_name"] + "/performance-unit-audit.json",
                    )
                )
                local = admit_fragment(fragment, unit, catalog, run, native)
            require(not (set(local) & set(values)), "duplicate tuple-case-role proof")
            association = {
                "reader_revision": reviewed_revision,
                "run_id": run["id"],
                "run_attempt": run["run_attempt"],
                "artifact_id": artifact["id"],
                "artifact_name": artifact["name"],
                "producing_job_id": job["id"],
                "producing_job": job["name"],
                "unit": key,
            }
            for value in local.values():
                value["reader_unit_association"] = association
            values.update(local)
            admitted.append(association)
        except auditor.EVIDENCE_ERRORS as error:
            errors.append({"fragment": number, "reason": str(error)})
    if seen != set(units):
        errors.append(
            {
                "reason": "missing original reader units",
                "missing": sorted(set(units) - seen),
            }
        )
    budget_error = None
    try:
        budgets = (
            auditor.validate_budgets(read(budgets_path), original["source_revision"])
            if budgets_path
            else None
        )
    except auditor.EVIDENCE_ERRORS as error:
        budgets, budget_error = None, str(error)
    report = auditor.report_from_values(
        original,
        source_run,
        source_completion,
        native,
        values,
        errors,
        budgets,
        budget_error,
    )
    report["reader_job_admission"] = {
        "status": "pass" if not errors and len(admitted) == len(units) else "fail",
        "reviewed_revision": reviewed_revision,
        "run_id": run["id"],
        "run_attempt": run["run_attempt"],
        "required_units": len(units),
        "admitted_units": admitted,
    }
    report["reader_completion"] = completion
    report["aggregate_revision"] = auditor.reader_identity()["revision"]
    report["acceptance_scope"] = (
        "numeric evidence qualification; migration closure additionally requires independently fetched actual aggregate/finalizer terminal-success run/job/artifact evidence after completion"
    )
    report["migration_acceptance"] = {
        "status": "pending",
        "reason": "the running aggregate cannot attest its own eventual successful completion or durable retention",
    }
    report["accepted"] = (
        report["accepted"]
        and report["reader_job_admission"]["status"] == "pass"
        and completion["status"] == "pass"
    )
    return report


def main(argv=None, entry_provider=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input", type=Path, required=True)
    parser.add_argument("--reviewed-auditor-revision", required=True)
    parser.add_argument("--budgets", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    checks = parser.add_mutually_exclusive_group()
    checks.add_argument("--check", action="store_true")
    checks.add_argument("--check-collection", action="store_true")
    args = parser.parse_args(argv)
    if args.check_collection and args.budgets:
        parser.error("collection-only aggregate forbids numeric budgets")
    args.output.mkdir(parents=True, exist_ok=False)
    try:
        report = aggregate(
            args.input, args.reviewed_auditor_revision, args.budgets, entry_provider
        )
    except auditor.EVIDENCE_ERRORS as error:
        report = {
            "protocol": "public-performance-audit/1",
            "accepted": False,
            "harness_correctness": {"status": "fail", "errors": [str(error)]},
            "coverage": {"status": "incomplete", "required": 567, "passed": 0},
            "comparator_coverage": {
                "status": "incomplete",
                "required": 504,
                "passed": 0,
            },
            "budgets": {"status": "not_evaluated"},
            "collection_gate": {"scope": "collection-only", "status": "fail"},
            "reader_job_admission": {"status": "fail"},
            "reader_completion": {"status": "incomplete"},
        }
    auditor.lifecycle.write_json(args.output / "performance-audit.json", report)
    text = (
        auditor.markdown(report)
        + f"\nReader job admission: {report['reader_job_admission']['status']}. Reader completion: {report['reader_completion']['status']}.\n"
        if "run_id" in report
        else f"# public performance aggregate\n\nRefused: {report['harness_correctness']['errors']}\n"
    )
    (args.output / "performance-audit.md").write_text(text, encoding="utf-8")
    return (
        1
        if (args.check and not report["accepted"])
        or (args.check_collection and report["collection_gate"]["status"] != "pass")
        else 0
    )


if __name__ == "__main__":
    raise SystemExit(main())
