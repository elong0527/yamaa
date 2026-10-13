"""Retained complete-public-run evidence; committed truth is never a fresh run."""

import importlib.util
import json
import shutil
import sys
from pathlib import Path


class _Reports:
    """Lazy ``_public_reports`` handle: its decoder libraries (Polars/PyArrow)
    are only present in qualification environments, so load on first use."""

    _module = None

    def _load(self):
        if self._module is None:
            spec = importlib.util.spec_from_file_location(
                "_public_reports", Path(__file__).with_name("_public_reports.py")
            )
            module = importlib.util.module_from_spec(spec)
            sys.modules[spec.name] = module
            spec.loader.exec_module(module)
            self._module = module
        return self._module

    def __getattr__(self, name):
        return getattr(self._load(), name)


reports = _Reports()

COHORT = (
    "adam-adlb-ordered-sum",
    "schema-lookup",
    "schema-window-functions",
    "schema-inheritance",
    "negative-zero-division",
    "negative-integer-overflow",
    "negative-formula-flag",
    "negative-row-aggregate",
    "negative-row-no-prior",
    "negative-source-missing-field",
    "negative-source-trivial-filter",
    "schema-verification-log",
)
PRODUCER = "shared-producer-parquet"
TRUTH_PATHS = {
    name: f"rust/crates/yamaa-adapters/tests/fixtures/specs/{name}.json"
    for name in COHORT
}
TRUTH_PATHS[PRODUCER] = (
    "rust/qualification/producer-cohort/shared-producer-parquet/reference.json"
)
PROTOCOL = "complete-public-cohort/2"
TRUTH_KIND = "committed-authored-report-truth"


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def exact_json(left, right):
    """Compare raw protocol values without Python's bool/int equivalence."""
    if type(left) is not type(right):
        return False
    if isinstance(left, dict):
        return left.keys() == right.keys() and all(
            exact_json(left[key], right[key]) for key in left
        )
    if isinstance(left, list):
        return len(left) == len(right) and all(
            exact_json(a, b) for a, b in zip(left, right)
        )
    return left == right


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, indent=2, ensure_ascii=True) + "\n", encoding="utf-8"
    )


def identity(name, revision):
    return {
        "kind": TRUTH_KIND,
        "source_revision": revision,
        "path": TRUTH_PATHS[name],
        "retained_path": f"truth/{name}.json",
    }


def retain_truth(checkout, destination, name, revision):
    record = identity(name, revision)
    source = checkout / record["path"]
    retained = destination / record["retained_path"]
    retained.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, retained)
    if retained.read_bytes() != source.read_bytes():
        raise ValueError("retaining complete-report truth changed committed bytes")
    return reports.read_report(retained), record


def batch(record, runtime, report_directory):
    installation = record["installation"]
    return {
        "runtime": runtime,
        "backend": "rust",
        "level": "shared_run",
        "source_revision": record["evidence"]["source_revision"],
        "host_package_version": installation["host_package_version"],
        "core_version": installation["core_version"],
        "binding_package_version": installation.get("binding_package_version"),
        "artifact_reference": record["evidence"]["artifact_reference"],
        "evidence": record["evidence"]["run"],
        "reports_dir": report_directory,
    }


def require_report_owner(report, record, runtime):
    installation = record["installation"]
    if (report.runtime, report.backend, report.engine_version) != (
        runtime,
        "rust",
        installation["core_version"],
    ):
        raise ValueError("actual complete report installed host/core differs")
    # Python transports sys.version or platform.python_version; R transports getRversion.
    version = report.runtime_version.split()
    if not version or version[0] != installation["runtime_version"]:
        raise ValueError(
            "actual complete report runtime version differs from installed owner"
        )


def coverage_row(name, runtime, truth, retained_report, verdicts):
    findings = [
        finding.model_dump(mode="json") for v in verdicts for finding in v.findings
    ]
    return {
        "example": name,
        "runtime": runtime,
        "backend": "rust",
        "level": "shared_run",
        "truth": truth,
        "report": retained_report,
        "result": "pass" if not findings else "semantic_mismatch",
        "findings": findings,
        "error": None,
    }


def inventory(revision, names, batches, coverage):
    result = {
        "protocol": PROTOCOL,
        "source_revision": revision,
        "batches": batches,
        "coverage": coverage,
        "errors": [],
        "summary": {
            host: sum(
                row["result"] == "pass" and row["runtime"] == host for row in coverage
            )
            for host in ("python", "r")
        },
    }
    validate_inventory(result, revision, names)
    return result


def validate_inventory(value, revision, names):
    if value.get("protocol") != PROTOCOL or value.get("source_revision") != revision:
        raise ValueError("cohort protocol/source differs")
    expected = {
        (name, host, "rust", "shared_run") for name in names for host in ("python", "r")
    }
    observed = {
        (row["example"], row["runtime"], row["backend"], row["level"])
        for row in value["coverage"]
    }
    if observed != expected or len(value["coverage"]) != len(expected):
        raise ValueError("cohort complete case/host tuples differ")
    for row in value["coverage"]:
        if row["truth"] != identity(row["example"], revision):
            raise ValueError("cohort committed truth revision/path/kind differs")
        expected_report = (
            f"reports/{row['runtime']}/{row['example']}.{row['runtime']}.rust.json"
        )
        if row["report"] != expected_report:
            raise ValueError("cohort retained report path differs")
        if row["result"] != "pass" or row["findings"] or row["error"]:
            raise ValueError("cohort complete comparisons do not pass")
    routes = {(b["runtime"], b["backend"], b["level"]) for b in value["batches"]}
    if len(value["batches"]) != 2 or routes != {
        ("python", "rust", "shared_run"),
        ("r", "rust", "shared_run"),
    }:
        raise ValueError("cohort needs both actual installed public owners")
    for owner in value["batches"]:
        if owner["source_revision"] != revision or not all(
            owner.get(k)
            for k in (
                "core_version",
                "host_package_version",
                "artifact_reference",
                "evidence",
            )
        ):
            raise ValueError("cohort installed batch provenance differs")
    if value["errors"] or value["summary"] != {
        host: len(names) for host in ("python", "r")
    }:
        raise ValueError("cohort errors/summary differs")


def require_same_run(python_record, r_record):
    if (
        python_record["platform"].split("-", 1)[0]
        != r_record["platform"].split("-", 1)[0]
    ):
        raise ValueError("public owners must share an OS family")
    if (
        python_record["installation"]["core_version"]
        != r_record["installation"]["core_version"]
        or python_record["evidence"]["run"] != r_record["evidence"]["run"]
    ):
        raise ValueError("public owners must share their actual core and Actions run")
