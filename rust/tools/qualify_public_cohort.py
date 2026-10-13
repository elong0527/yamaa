"""Compare actual installed complete public reports with unchanged committed truth.

No reference evaluator or assisted inventory is imported or executed. Coverage v2
identifies authored report truth by exact Git revision/path, separately from the
actual installed host owners and their retained raw reports.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import shutil
import subprocess
from pathlib import Path

_spec = importlib.util.spec_from_file_location(
    "_cohort_evidence", Path(__file__).with_name("_cohort_evidence.py")
)
cohort = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(cohort)
COHORT = cohort.COHORT
ROOT = Path(__file__).resolve().parents[2]
read = cohort.read


def installed_evidence(directory, runtime, source_revision):
    supplemental = directory / "supplemental" if runtime == "python" else directory
    record = read(supplemental / "supplemental.json")
    evidence = record["evidence"]
    if evidence["source_revision"] != source_revision:
        raise ValueError("stale installed public-run evidence")
    suites = {s["id"]: s for s in record["suites"]}
    for suffix in ("original-specifications", "verification-logs"):
        suite = suites[runtime + "/" + suffix]
        if suite["result"] != "pass" or suite["exit_code"] != 0:
            raise ValueError("public original-document suite did not pass: " + suffix)
        log = (supplemental / suite["log"]).read_text()
        if runtime == "python" and "skipped=" in log:
            raise ValueError("skipped public file runs cannot qualify the cohort")
        for name in COHORT if runtime == "r" else ():
            if (name == "schema-verification-log") != (suffix == "verification-logs"):
                continue
            witness = (
                name
                + " public domain complete Rust run report and successful save observations passed"
            )
            if witness not in log:
                raise ValueError("missing public R complete-run witness: " + name)
    return record, cohort.batch(record, runtime, "reports/" + runtime)


def compare_owners(
    fixtures, python_evidence, r_evidence, output, revision, checkout=ROOT
):
    python_record, python_batch = installed_evidence(
        python_evidence, "python", revision
    )
    r_record, r_batch = installed_evidence(r_evidence, "r", revision)
    cohort.require_same_run(python_record, r_record)
    output.mkdir(parents=True)
    for runtime, directory in (("python", python_evidence), ("r", r_evidence)):
        supplemental = directory / "supplemental" if runtime == "python" else directory
        shutil.copyfile(
            supplemental / "supplemental.json",
            output / (runtime + "-installed-evidence.json"),
        )
    rows = []
    for name in COHORT:
        truth, truth_identity = cohort.retain_truth(checkout, output, name, revision)
        for runtime, directory, owner in (
            ("python", python_evidence, python_batch),
            ("r", r_evidence, r_batch),
        ):
            filename = f"{name}.{runtime}.rust.json"
            relative = f"reports/{runtime}/{filename}"
            retained = output / relative
            retained.parent.mkdir(parents=True, exist_ok=True)
            source = directory / "public-reports" / filename
            shutil.copyfile(source, retained)
            if retained.read_bytes() != source.read_bytes():
                raise ValueError("retaining actual complete report changed bytes")
            actual = cohort.reports.read_report(retained)
            if actual.error is not None:
                raise ValueError(
                    "actual public complete report retains an infrastructure error"
                )
            if (
                actual.example,
                actual.runtime,
                actual.backend,
                actual.engine_version,
            ) != (name, runtime, "rust", owner["core_version"]):
                raise ValueError(
                    "actual complete report identity/installed core differs"
                )
            verdicts = (
                cohort.reports.compare_reports(truth, actual),
                cohort.reports.compare_example(actual, fixtures / name),
            )
            rows.append(
                cohort.coverage_row(name, runtime, truth_identity, relative, verdicts)
            )
    inventory = cohort.inventory(revision, COHORT, [python_batch, r_batch], rows)
    cohort.write(output / "coverage.json", inventory)
    return inventory


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
        parser.error("output must be fresh and outside the committed fixture tree")
    if args.fixtures.resolve() != (ROOT / "benchmarks").resolve():
        parser.error("complete-report truth requires the reviewed benchmark checkout")
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
