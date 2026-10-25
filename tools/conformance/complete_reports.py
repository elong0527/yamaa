"""Compare actual installed complete reports with committed authored truth.

Authored report truth, complete installed observations and saved bytes remain
independent inputs. No candidate engine or reference evaluator is imported.
The command line's run of the same cases joins the comparison with what it
saved: its outcomes and saved files must equal both installed hosts'.
"""

import json
import shutil
from pathlib import PurePosixPath

from tools.conformance import report_values as reports
from tools.paths import ROOT


def read(path):
    if path.stat().st_size > 16_777_216:
        raise ValueError("complete report evidence exceeds its report budget")
    return json.loads(path.read_text(encoding="utf-8"))


def public_cases(checkout=ROOT):
    """Discover complete authored reports; partial diagnostic fixtures are arrays."""
    names = []
    directory = checkout / "rust/crates/yamaa-application/tests/fixtures/specs"
    for path in sorted(directory.glob("*.json")):
        raw = read(path)
        if isinstance(raw, list):
            continue
        # Objects in this directory are complete truth. A malformed report
        # must fail rather than disappearing from discovered qualification.
        report = reports.parse_report(json.dumps(raw))
        if report.example != path.stem:
            raise ValueError("authored report filename/example differs: " + path.name)
        names.append(path.stem)
    if not names:
        raise ValueError("no authored complete reports")
    return tuple(names)


COHORT = public_cases()

TRUTH_PATHS = {
    name: f"rust/crates/yamaa-application/tests/fixtures/specs/{name}.json"
    for name in COHORT
}
PROTOCOL = "installed-complete-reports/1"
TRUTH_KIND = "committed-authored-report-truth"


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


def installed_owner(directory, runtime, revision):
    supplemental = directory / "supplemental" if runtime == "python" else directory
    record = read(supplemental / "supplemental.json")
    if record["evidence"]["source_revision"] != revision:
        raise ValueError("stale installed complete-report evidence")
    suites = {suite["id"]: suite for suite in record["suites"]}
    # The run-report suite owns schema-verification-log, the cohort case
    # that declares output.report; the original suite owns the others.
    for suffix in ("original-specifications", "run-report"):
        suite = suites.get(runtime + "/" + suffix)
        if suite is None:
            raise ValueError(f"installed public suite is missing: {runtime}/{suffix}")
        if suite["result"] != "pass" or suite["exit_code"] != 0:
            raise ValueError("installed public suite did not pass: " + suffix)
        log = (supplemental / suite["log"]).read_text(encoding="utf-8")
        if runtime == "python" and "skipped=" in log:
            raise ValueError("skipped public execution cannot qualify complete reports")
        if runtime != "r":
            continue
        witnesses = [
            name
            + " public domain complete Rust run report and successful save observations passed"
            for name in COHORT
            if (name == "schema-verification-log") == (suffix == "run-report")
        ]
        for witness in witnesses:
            if witness not in log:
                raise ValueError("missing installed R complete-run witness: " + witness)
    return record


def compare_owners(
    fixtures, python_evidence, r_evidence, output, revision, checkout=ROOT
):
    directories = {"python": python_evidence, "r": r_evidence}
    owners = {
        host: installed_owner(directory, host, revision)
        for host, directory in directories.items()
    }
    require_same_run(owners["python"], owners["r"])
    output.mkdir(parents=True)
    for host, directory in directories.items():
        supplemental = directory / "supplemental" if host == "python" else directory
        shutil.copyfile(
            supplemental / "supplemental.json",
            output / (host + "-installed-evidence.json"),
        )
    rows, comparisons = [], []
    for name in COHORT:
        truth, truth_identity = retain_truth(checkout, output, name, revision)
        for host, directory in directories.items():
            filename = f"{name}.{host}.rust.json"
            relative = f"reports/{host}/{filename}"
            retained = output / relative
            retained.parent.mkdir(parents=True, exist_ok=True)
            source = directory / "public-reports" / filename
            shutil.copyfile(source, retained)
            if retained.read_bytes() != source.read_bytes():
                raise ValueError("retaining actual complete report changed bytes")
            actual = reports.read_report(retained)
            if actual.error is not None:
                raise ValueError(
                    "actual complete report retains an infrastructure error"
                )
            require_report_owner(actual, owners[host], host)
            if actual.example != name:
                raise ValueError("actual complete report example differs")
            verdicts = (
                reports.compare_reports(truth, actual),
                reports.compare_example(actual, fixtures / name),
            )
            rows.append(coverage_row(name, host, truth_identity, relative, verdicts))
            comparisons.extend(verdicts)
    batches = [batch(owners[host], host, "reports/" + host) for host in directories]
    result = inventory(revision, COHORT, batches, rows)
    write(output / "coverage.json", result)
    write(
        output / "complete-comparisons.json",
        [verdict.model_dump(mode="json") for verdict in comparisons],
    )
    return result


CLI_PROTOCOL = "installed-cli-saved-outputs/1"
HOSTS = ("python", "r")


def cli_run(directory, revision, owners):
    """The command line's conformance run, bound to the installed owners.

    It names this revision and the owners' Actions run, its binary prints the
    owners' core version, and its own comparison with the expected files
    passed every benchmark that the command line runs.
    """
    run = read(directory / "observed" / "run.json")
    inventory = read(directory / "inventory.json")
    if run.get("runtime") != "cli" or run.get("source_revision") != revision:
        raise ValueError("stale or foreign command-line evidence")
    if (
        inventory.get("runtime") != "cli"
        or inventory.get("source_revision") != revision
        or inventory.get("gate", {}).get("failing") != []
    ):
        raise ValueError("command-line conformance did not pass on this revision")
    version = (run.get("version") or "").split("\n", 1)[0]
    for host, owner in owners.items():
        if version != "yamaa " + owner["installation"]["core_version"]:
            raise ValueError(f"the command line's engine differs from the {host} core")
        if run.get("evidence") != owner["evidence"]["run"]:
            raise ValueError(f"the command line and the {host} owner ran apart")
    return run


def cli_saved(directory, destination, name, record):
    """One case's command-line outcome and the files its runs saved, in
    publication order, each retained under `destination` with its bytes."""
    if record.get("error") or not record.get("calls"):
        raise ValueError(f"no command-line run of {name}: {record.get('error')}")
    outcome, saved = "success", []
    for call in record["calls"]:
        document = call["document"]
        if (call["exit_status"], document["status"]) != (0, "succeeded"):
            outcome = "failure"
        for path in document["paths"]:
            relative = PurePosixPath(path)
            if relative.is_absolute() or ".." in relative.parts or "\\" in path:
                raise ValueError(f"{name}: a saved path is outside its case: {path}")
            source = directory / "observed" / "work" / name / path
            retained = destination / name / path
            retained.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, retained)
            payload = retained.read_bytes()
            if payload != source.read_bytes():
                raise ValueError("retaining a command-line saved file changed bytes")
            saved.append((path, payload))
    return outcome, saved


def saved_output_findings(report, outcome, saved):
    """What differs between one installed host's complete report and the
    command line's run of the same case: the outcome, then the saved
    artifacts in publication order, each CSV byte for byte and each Parquet
    file logically (PARQUET-4). The command line's run stands where committed
    truth stands in the hosts' comparisons, so each finding's expected value
    is the command line's and its actual value the host's."""
    findings = []
    if report.outcome != outcome:
        findings.append(
            reports.ComparisonFinding(
                kind="outcome",
                detail=f"{report.example}: outcomes differ",
                expected=outcome,
                actual=report.outcome,
            )
        )
    produced = [f"{item.name}.{item.profile}" for item in report.artifacts]
    names = [PurePosixPath(path).name for path, _ in saved]
    if produced != names:
        findings.append(
            reports.ComparisonFinding(
                kind="artifacts",
                detail=f"{report.example}: saved artifacts differ",
                expected=names,
                actual=produced,
            )
        )
        return findings
    for artifact, (_, payload) in zip(report.artifacts, saved):
        findings.extend(reports._artifact_findings(artifact, payload))
    return findings


def compare_cli_outputs(cli_evidence, output, revision):
    """Compare the command line's saved outputs with the installed hosts'.

    `output` holds what compare_owners retained: both installed owners'
    evidence and their complete reports of every cohort case. The command
    line ran the same cases, written as 1.0 (#3030 D122), so its spec paths
    are the 1.0 files' and its issues are not compared here; its outcome and
    its saved files are, with both hosts' reports. Its own inventory proved
    its runs against the expected files.
    """
    owners = {host: read(output / f"{host}-installed-evidence.json") for host in HOSTS}
    run = cli_run(cli_evidence, revision, owners)
    destination = output / "cli"
    destination.mkdir()
    shutil.copyfile(cli_evidence / "observed" / "run.json", destination / "run.json")
    shutil.copyfile(cli_evidence / "inventory.json", destination / "inventory.json")
    rows = []
    for name in COHORT:
        record = read(cli_evidence / "observed" / "records" / f"{name}.json")
        if record.get("excluded"):
            rows.extend(
                {
                    "example": name,
                    "compared_with": host,
                    "result": "excluded",
                    "reason": record["excluded"],
                    "findings": [],
                }
                for host in HOSTS
            )
            continue
        outcome, saved = cli_saved(cli_evidence, destination / "saved", name, record)
        for host in HOSTS:
            relative = f"reports/{host}/{name}.{host}.rust.json"
            findings = saved_output_findings(
                reports.read_report(output / relative), outcome, saved
            )
            rows.append(
                {
                    "example": name,
                    "compared_with": host,
                    "report": relative,
                    "outcome": outcome,
                    "saved": [f"cli/saved/{name}/{path}" for path, _ in saved],
                    "result": "mismatch" if findings else "pass",
                    "findings": [item.model_dump(mode="json") for item in findings],
                }
            )
    result = {
        "protocol": CLI_PROTOCOL,
        "source_revision": revision,
        "cli": {key: run.get(key) for key in ("version", "evidence")},
        "coverage": rows,
        "summary": {
            host: sum(
                row["compared_with"] == host and row["result"] == "pass" for row in rows
            )
            for host in HOSTS
        },
    }
    write(destination / "coverage.json", result)
    differing = sorted(
        {row["example"] for row in rows if row["result"] not in ("pass", "excluded")}
    )
    if differing:
        raise ValueError(
            "the command line's saved outputs differ from the installed hosts': "
            + ", ".join(differing)
        )
    return result
