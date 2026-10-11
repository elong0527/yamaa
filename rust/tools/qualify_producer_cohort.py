"""Compare complete installed producer owners and saved bytes through M1.

Execution happens in the separate installed-host suites. This tool retains raw
owner evidence, projects only protocol identity metadata, and uses the existing
independent report/artifact comparators. It never invokes a candidate engine.
"""

from __future__ import annotations

import argparse
import copy
import json
import shutil
from pathlib import Path, PurePosixPath

from yamaa.adapters.conformance import (
    ExampleReport,
    compare_example,
    compare_reports,
    write_report,
)
from yamaa.adapters.qualification import Batch, load_inventory, qualify

NAME = "shared-producer-parquet"
TARGETS = ("produced.parquet", "out.csv")


def read(path):
    if path.stat().st_size > 16_777_216:
        raise ValueError("producer evidence exceeds its report budget")
    return json.loads(path.read_text(encoding="utf-8"))


def relative_identity(value, case):
    # Compare lexical canonical identities across the host OS. The saved case
    # may no longer exist, and Windows reports are also audited on Linux.
    return PurePosixPath(value).relative_to(PurePosixPath(case)).as_posix()


def project_report(raw, case):
    fields = set(ExampleReport.model_fields)
    if set(raw) - fields - {"entry", "activation", "environment", "publication"}:
        raise ValueError("unknown native producer report fields")
    if raw["backend"] != "rust" or raw["example"] != "domain" or raw["error"] is not None:
        raise ValueError("actual successful public producer report required")
    if raw["entry"] != "spec.yaml" or raw["activation"] != {
        "lock": "not_requested", "bindings": [], "tests": [],
    }:
        raise ValueError("unexpected producer entry or activation ledger")
    protocol = copy.deepcopy({key: raw[key] for key in fields})
    protocol["example"] = NAME
    for group in ("nodes", "tables", "verifications", "callbacks"):
        for observation in protocol[group]:
            observation["specification"] = relative_identity(observation["specification"], case)
    for observation in protocol["source_reads"]:
        if relative_identity(observation.pop("specification"), case) not in {"producer.yaml", "spec.yaml"}:
            raise ValueError("resource request is not from an original graph node")
        observation["base_directory"] = relative_identity(observation["base_directory"], case)
    owners = {"produced": "producer.yaml", "out": "spec.yaml"}
    for artifact in protocol["artifacts"]:
        if relative_identity(artifact.pop("specification"), case) != owners[artifact["name"]]:
            raise ValueError("artifact does not belong to its actual node")
    return ExampleReport.model_validate_json(json.dumps(protocol))


def require_parity(reference, candidate, label):
    verdict = compare_reports(reference, candidate)
    if not verdict.passed:
        raise ValueError(label + ": " + verdict.model_dump_json())
    return verdict


def installed_record(directory, runtime, source_revision):
    supplemental = directory / "supplemental" if runtime == "python" else directory
    record = read(supplemental / "supplemental.json")
    if record["evidence"]["source_revision"] != source_revision:
        raise ValueError("stale installed producer evidence")
    suite = next(item for item in record["suites"] if item["id"] == runtime + "/producer-cohort")
    if suite["result"] != "pass" or suite["exit_code"] != 0:
        raise ValueError("installed producer cohort did not pass")
    log = (supplemental / suite["log"]).read_text(encoding="utf-8")
    if runtime == "python" and "skipped=" in log:
        raise ValueError("skipped producer execution cannot qualify")
    if runtime == "r" and NAME + " actual installed public owner and repeated held-byte Save evidence passed" not in log:
        raise ValueError("R public producer witness is missing")
    return record


def retain_owner(directory, runtime, record, expected, output):
    source = directory / "public-reports/producer-cohort" / runtime / NAME
    provenance = read(source / "provenance.json")
    if provenance != {
        "scope": "supplemental_suites", "evidence": record["evidence"], "installation": record["installation"],
    }:
        raise ValueError("producer owner source/artifact provenance differs")
    raw_directory = output / "raw" / runtime / NAME
    shutil.copytree(source, raw_directory)
    case = (raw_directory / "case-directory.txt").read_text(encoding="utf-8")
    before = read(raw_directory / "before.json")
    if "publication" in before:
        raise ValueError("Build unexpectedly contains a publication attempt")
    before_report = project_report(before, case)
    require_parity(expected, before_report, runtime + " original complete Build report")
    snapshots = []
    comparisons = []
    for attempt in (1, 2):
        saved = raw_directory / f"after-{attempt}"
        after = read(saved / "report.json")
        publication = after.pop("publication")
        if publication != {
            "status": "success", "error": None,
            "entered": [{"node": 1, "published": True}, {"node": 0, "published": True}],
        }:
            raise ValueError("producer publication does not retain its exact successful prefix")
        if after != before:
            raise ValueError("Save changed original execution or source evidence")
        projected = project_report(after, case)
        comparisons.append(require_parity(expected, projected, runtime + " complete Save report"))
        actual = {name: (saved / name).read_bytes() for name in TARGETS}
        artifacts = {artifact.name: artifact for artifact in projected.artifacts}
        for name, payload in actual.items():
            artifact = artifacts[Path(name).stem]
            if len(payload) != artifact.byte_length:
                raise ValueError("saved artifact length differs from its actual held output")
            if artifact.profile == "csv" and payload != artifact.content.encode("utf-8"):
                raise ValueError("saved CSV bytes differ from actual held output")
        published_fixture = output / "published" / runtime / str(attempt)
        (published_fixture / "expected").mkdir(parents=True)
        for name, payload in actual.items():
            (published_fixture / "expected" / name).write_bytes(payload)
        verdict = compare_example(projected, published_fixture)
        if not verdict.passed:
            raise ValueError("saved producer bytes: " + verdict.model_dump_json())
        snapshots.append(actual)
    if snapshots[0] != snapshots[1]:
        raise ValueError("repeat Save changed held artifact bytes")
    reports = output / "reports" / runtime
    write_report(projected, reports)
    installation = record["installation"]
    batch = Batch(
        runtime=runtime, backend="rust", level="shared_run",
        source_revision=record["evidence"]["source_revision"],
        host_package_version=installation["host_package_version"],
        core_version=installation["core_version"],
        binding_package_version=installation.get("binding_package_version"),
        artifact_reference=record["evidence"]["artifact_reference"],
        evidence=record["evidence"]["run"], reports_dir=str(reports.resolve()),
    )
    return batch, comparisons


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--python-evidence", type=Path, required=True)
    parser.add_argument("--r-evidence", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-revision", required=True)
    args = parser.parse_args(argv)
    if args.output.exists() or args.output.resolve().is_relative_to(args.fixtures.resolve()):
        parser.error("output must be fresh and outside the original fixture tree")
    if set(load_inventory(args.fixtures).examples) != {NAME}:
        raise ValueError("unexpected original producer cohort inventory")
    python = installed_record(args.python_evidence, "python", args.source_revision)
    r = installed_record(args.r_evidence, "r", args.source_revision)
    if python["platform"].split("-", 1)[0] != r["platform"].split("-", 1)[0]:
        raise ValueError("producer hosts must share an OS family")
    if python["installation"]["core_version"] != r["installation"]["core_version"] or python["evidence"]["run"] != r["evidence"]["run"]:
        raise ValueError("producer hosts must share the actual core and qualification run")
    records = [("python", args.python_evidence, python), ("r", args.r_evidence, r)]
    reference = Batch.model_validate_json((args.python_evidence / "producer-reference-provenance.json").read_text(encoding="utf-8"))
    if (reference.runtime, reference.backend, reference.level, reference.source_revision) != ("python", "python", "reference_run", args.source_revision):
        raise ValueError("fresh independent producer reference evidence required")
    if reference.evidence != python["evidence"]["run"] or reference.artifact_reference != python["evidence"]["host_artifact_reference"]:
        raise ValueError("independent producer reference source/artifact provenance differs")
    expected = ExampleReport.model_validate_json((args.fixtures / NAME / "reference.json").read_text(encoding="utf-8"))
    fresh = ExampleReport.model_validate_json((args.python_evidence / "producer-reference" / f"{NAME}.python.python.json").read_text(encoding="utf-8"))
    require_parity(expected, fresh, "independent original reference report")
    if not compare_example(fresh, args.fixtures / NAME).passed:
        raise ValueError("independent original producer artifact truth differs")
    args.output.mkdir(parents=True)
    shutil.copytree(args.fixtures, args.output / "fixtures")
    reference_directory = args.output / "reports/reference"
    write_report(fresh, reference_directory)
    batches = [reference.model_copy(update={"reports_dir": str(reference_directory.resolve())})]
    comparisons = []
    for runtime, directory, record in records:
        batch, verdicts = retain_owner(directory, runtime, record, expected, args.output)
        batches.append(batch)
        comparisons.extend(verdicts)
    required = ((NAME, "python", "python", "reference_run"), (NAME, "python", "rust", "shared_run"), (NAME, "r", "rust", "shared_run"))
    inventory = qualify(args.output / "fixtures", args.source_revision, tuple(batches), required=required)
    (args.output / "coverage.json").write_text(inventory.model_dump_json(indent=2) + "\n", encoding="utf-8")
    (args.output / "complete-comparisons.json").write_text(json.dumps([verdict.model_dump(mode="json") for verdict in comparisons], indent=2) + "\n", encoding="utf-8")
    print(json.dumps(inventory.summary, indent=2))
    for error in inventory.errors:
        print(error)
    return int(bool(inventory.errors))


if __name__ == "__main__":
    raise SystemExit(main())
