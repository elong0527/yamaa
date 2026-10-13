"""Compare complete installed producer owners and saved bytes through M1.

Execution happens in the separate installed-host suites. This tool retains raw
owner evidence, projects only protocol identity metadata, and uses the tool-private
independent report/artifact comparators and unchanged committed report truth. It never invokes a candidate engine.
"""

from __future__ import annotations

import argparse
import copy
import importlib.util
import json
import shutil
import subprocess
from pathlib import Path, PurePosixPath

_spec = importlib.util.spec_from_file_location(
    "_cohort_evidence", Path(__file__).with_name("_cohort_evidence.py")
)
cohort = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cohort)
ExampleReport = cohort.reports.ExampleReport
compare_example = cohort.reports.compare_example
compare_reports = cohort.reports.compare_reports
write_report = cohort.reports.write_report
ROOT = Path(__file__).resolve().parents[2]

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
    if set(raw) != fields | {"entry", "activation", "environment"}:
        raise ValueError("unknown or incomplete native producer report fields")
    if (
        raw["backend"] != "rust"
        or raw["example"] != "domain"
        or raw["error"] is not None
    ):
        raise ValueError("actual successful public producer report required")
    if raw["entry"] != "spec.yaml" or raw["activation"] != {
        "lock": "not_requested",
        "bindings": [],
        "tests": [],
    }:
        raise ValueError("unexpected producer entry or activation ledger")
    environment = raw["environment"]
    parent = PurePosixPath(case).parent
    lock = "uv.lock" if raw["runtime"] == "python" else "renv.lock"
    if environment != {
        "source": (parent / "environment.yaml").as_posix(),
        "lock_source": (parent / lock).as_posix(),
    }:
        raise ValueError("producer environment is not from its actual retained owner")
    protocol = copy.deepcopy({key: raw[key] for key in fields})
    protocol["example"] = NAME
    for group in ("nodes", "tables", "verifications", "callbacks"):
        for observation in protocol[group]:
            observation["specification"] = relative_identity(
                observation["specification"], case
            )
    for observation in protocol["source_reads"]:
        if relative_identity(observation.pop("specification"), case) not in {
            "producer.yaml",
            "spec.yaml",
        }:
            raise ValueError("resource request is not from an original graph node")
        observation["base_directory"] = relative_identity(
            observation["base_directory"], case
        )
    owners = {"produced": "producer.yaml", "out": "spec.yaml"}
    for artifact in protocol["artifacts"]:
        if (
            relative_identity(artifact.pop("specification"), case)
            != owners[artifact["name"]]
        ):
            raise ValueError("artifact does not belong to its actual node")
    return cohort.reports.parse_report(json.dumps(protocol))


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
    suite = next(
        item for item in record["suites"] if item["id"] == runtime + "/producer-cohort"
    )
    if suite["result"] != "pass" or suite["exit_code"] != 0:
        raise ValueError("installed producer cohort did not pass")
    log = (supplemental / suite["log"]).read_text(encoding="utf-8")
    if runtime == "python" and "skipped=" in log:
        raise ValueError("skipped producer execution cannot qualify")
    if (
        runtime == "r"
        and NAME
        + " actual installed public owner and repeated held-byte Save evidence passed"
        not in log
    ):
        raise ValueError("R public producer witness is missing")
    return record


def retain_owner(directory, runtime, record, expected, output):
    source = directory / "public-reports/producer-cohort" / runtime / NAME
    provenance = read(source / "provenance.json")
    if provenance != {
        "scope": "supplemental_suites",
        "evidence": record["evidence"],
        "installation": record["installation"],
    }:
        raise ValueError("producer owner source/artifact provenance differs")
    raw_directory = output / "raw" / runtime / NAME
    shutil.copytree(source, raw_directory)
    case = (raw_directory / "case-directory.txt").read_text(encoding="utf-8")
    before = read(raw_directory / "before.json")
    if "publication" in before:
        raise ValueError("Build unexpectedly contains a publication attempt")
    before_report = project_report(before, case)
    if (
        before_report.runtime != runtime
        or before_report.engine_version != record["installation"]["core_version"]
    ):
        raise ValueError("producer report differs from its actual installed host/core")
    require_parity(expected, before_report, runtime + " original complete Build report")
    snapshots = []
    comparisons = []
    for attempt in (1, 2):
        saved = raw_directory / f"after-{attempt}"
        after = read(saved / "report.json")
        publication = after.pop("publication")
        if publication != {
            "status": "success",
            "error": None,
            "entered": [{"node": 1, "published": True}, {"node": 0, "published": True}],
        }:
            raise ValueError(
                "producer publication does not retain its exact successful prefix"
            )
        if after != before:
            raise ValueError("Save changed original execution or source evidence")
        projected = project_report(after, case)
        comparisons.append(
            require_parity(expected, projected, runtime + " complete Save report")
        )
        actual = {name: (saved / name).read_bytes() for name in TARGETS}
        artifacts = {artifact.name: artifact for artifact in projected.artifacts}
        for name, payload in actual.items():
            artifact = artifacts[Path(name).stem]
            if len(payload) != artifact.byte_length:
                raise ValueError(
                    "saved artifact length differs from its actual held output"
                )
            if artifact.profile == "csv" and payload != artifact.content.encode(
                "utf-8"
            ):
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
    batch = cohort.batch(record, runtime, "reports/" + runtime)

    return batch, comparisons


def compare_owners(
    fixtures, python_evidence, r_evidence, output, revision, checkout=ROOT
):
    python = installed_record(python_evidence, "python", revision)
    r = installed_record(r_evidence, "r", revision)
    cohort.require_same_run(python, r)
    output.mkdir(parents=True)
    truth, truth_identity = cohort.retain_truth(checkout, output, NAME, revision)
    shutil.copytree(fixtures, output / "fixtures")
    comparisons = []
    rows = []
    batches = []
    for runtime, directory, record in (
        ("python", python_evidence, python),
        ("r", r_evidence, r),
    ):
        batch, verdicts = retain_owner(directory, runtime, record, truth, output)
        batches.append(batch)
        comparisons.extend(verdicts)
        retained = f"reports/{runtime}/{NAME}.{runtime}.rust.json"
        rows.append(
            cohort.coverage_row(NAME, runtime, truth_identity, retained, verdicts)
        )
        supplemental = directory / "supplemental" if runtime == "python" else directory
        shutil.copyfile(
            supplemental / "supplemental.json",
            output / (runtime + "-installed-evidence.json"),
        )
    result = cohort.inventory(revision, (NAME,), batches, rows)
    cohort.write(output / "coverage.json", result)
    cohort.write(
        output / "complete-comparisons.json",
        [verdict.model_dump(mode="json") for verdict in comparisons],
    )
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--python-evidence", type=Path, required=True)
    parser.add_argument("--r-evidence", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-revision", required=True)
    args = parser.parse_args(argv)
    if args.output.exists() or args.output.resolve().is_relative_to(
        args.fixtures.resolve()
    ):
        parser.error("output must be fresh and outside the original fixture tree")
    if (
        args.fixtures.resolve()
        != (ROOT / "rust/qualification/producer-cohort").resolve()
    ):
        parser.error("complete producer truth requires the reviewed fixture checkout")
    actual = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD"], text=True
    ).strip()
    if args.source_revision != actual:
        parser.error("candidate source revision differs from the reviewed checkout")
    result = compare_owners(
        args.fixtures, args.python_evidence, args.r_evidence, args.output, actual
    )
    print(json.dumps(result["summary"], indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
