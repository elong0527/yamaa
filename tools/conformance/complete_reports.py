"""Compare actual installed complete reports and repeated producer Save evidence.

Authored report truth, complete installed observations and saved bytes remain
independent inputs. No candidate engine or reference evaluator is imported.
"""

import copy
import json
import shutil
from pathlib import Path, PurePosixPath

from tools.conformance import report_values as reports
from tools.paths import ROOT


def read(path):
    if path.stat().st_size > 16_777_216:
        raise ValueError("complete report evidence exceeds its report budget")
    return json.loads(path.read_text(encoding="utf-8"))


def public_cases(checkout=ROOT):
    """Discover complete authored reports; partial diagnostic fixtures are arrays."""
    names = []
    directory = checkout / "rust/crates/yamaa-adapters/tests/fixtures/specs"
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

PRODUCER = "shared-producer-parquet"
TRUTH_PATHS = {
    name: f"rust/crates/yamaa-adapters/tests/fixtures/specs/{name}.json"
    for name in COHORT
}
TRUTH_PATHS[PRODUCER] = (
    "rust/qualification/producer-cohort/shared-producer-parquet/reference.json"
)
PROTOCOL = "installed-complete-reports/1"
TRUTH_KIND = "committed-authored-report-truth"


TARGETS = ("produced.parquet", "out.csv")


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


def relative_identity(value, case):
    # Compare lexical canonical identities across the host OS. The saved case
    # may no longer exist, and Windows reports are also audited on Linux.
    return PurePosixPath(value).relative_to(PurePosixPath(case)).as_posix()


def project_report(raw, case):
    fields = set(reports.ExampleReport.model_fields)
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
    # The report's environment section names the project file the owner
    # passed and the lock its linked environment file declares, both beside
    # the retained case.
    environment = raw["environment"]
    parent = PurePosixPath(case).parent
    lock = "uv.lock" if raw["runtime"] == "python" else "renv.lock"
    if environment != {
        "source": (parent / "project.yaml").as_posix(),
        "lock_source": (parent / lock).as_posix(),
    }:
        raise ValueError(
            "producer project or lock is not from its actual retained owner"
        )
    protocol = copy.deepcopy({key: raw[key] for key in fields})
    protocol["example"] = PRODUCER
    for group in ("nodes", "tables", "verifications", "callbacks"):
        for observation in protocol[group]:
            observation["specification"] = relative_identity(
                observation["specification"], case
            )
    for observation in protocol["source_reads"]:
        if relative_identity(observation.pop("specification"), case) != "producer.yaml":
            raise ValueError(
                "seed resource request is not owned by its original producer"
            )
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
    return reports.parse_report(json.dumps(protocol))


def require_parity(reference, candidate, label):
    verdict = reports.compare_reports(reference, candidate)
    if not verdict.passed:
        raise ValueError(label + ": " + verdict.model_dump_json())
    return verdict


def retain_producer(directory, runtime, record, expected, output):
    source = directory / "public-reports/producer-cohort" / runtime / PRODUCER
    provenance = read(source / "provenance.json")
    if provenance != {
        "scope": "supplemental_suites",
        "evidence": record["evidence"],
        "installation": record["installation"],
    }:
        raise ValueError("producer owner source/artifact provenance differs")
    raw_directory = output / "raw" / runtime / PRODUCER
    shutil.copytree(source, raw_directory)
    case = (raw_directory / "case-directory.txt").read_text(encoding="utf-8")
    before = read(raw_directory / "before.json")
    if "publication" in before:
        raise ValueError("Build unexpectedly contains a publication attempt")
    before_report = project_report(before, case)
    require_report_owner(before_report, record, runtime)
    require_parity(expected, before_report, runtime + " original complete Build report")
    snapshots = []
    comparisons = []
    for attempt in (1, 2):
        saved = raw_directory / f"after-{attempt}"
        after = read(saved / "report.json")
        publication = after.pop("publication")
        if not exact_json(
            publication,
            {
                "status": "success",
                "error": None,
                "entered": [
                    {"node": 1, "published": True},
                    {"node": 0, "published": True},
                ],
            },
        ):
            raise ValueError(
                "producer publication does not retain its exact successful prefix"
            )
        if not exact_json(after, before):
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
        verdict = reports.compare_example(projected, published_fixture)
        if not verdict.passed:
            raise ValueError("saved producer bytes: " + verdict.model_dump_json())
        snapshots.append(actual)
    if snapshots[0] != snapshots[1]:
        raise ValueError("repeat Save changed held artifact bytes")
    report_directory = output / "reports" / runtime
    reports.write_report(projected, report_directory)
    return comparisons


def installed_owner(directory, runtime, revision):
    supplemental = directory / "supplemental" if runtime == "python" else directory
    record = read(supplemental / "supplemental.json")
    if record["evidence"]["source_revision"] != revision:
        raise ValueError("stale installed complete-report evidence")
    suites = {suite["id"]: suite for suite in record["suites"]}
    for suffix in ("original-specifications", "verification-logs", "producer-cohort"):
        suite = suites[runtime + "/" + suffix]
        if suite["result"] != "pass" or suite["exit_code"] != 0:
            raise ValueError("installed public suite did not pass: " + suffix)
        log = (supplemental / suite["log"]).read_text(encoding="utf-8")
        if runtime == "python" and "skipped=" in log:
            raise ValueError("skipped public execution cannot qualify complete reports")
        if runtime != "r":
            continue
        if suffix == "producer-cohort":
            witnesses = [
                PRODUCER
                + " actual installed public owner and repeated held-byte Save evidence passed"
            ]
        else:
            witnesses = [
                name
                + " public domain complete Rust run report and successful save observations passed"
                for name in COHORT
                if (name == "schema-verification-log")
                == (suffix == "verification-logs")
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
    truth, truth_identity = retain_truth(checkout, output, PRODUCER, revision)
    shutil.copytree(
        checkout / "rust/qualification/producer-cohort", output / "fixtures"
    )
    for host, directory in directories.items():
        verdicts = retain_producer(directory, host, owners[host], truth, output)
        rows.append(
            coverage_row(
                PRODUCER,
                host,
                truth_identity,
                f"reports/{host}/{PRODUCER}.{host}.rust.json",
                verdicts,
            )
        )
        comparisons.extend(verdicts)
    batches = [batch(owners[host], host, "reports/" + host) for host in directories]
    result = inventory(revision, (*COHORT, PRODUCER), batches, rows)
    write(output / "coverage.json", result)
    write(
        output / "complete-comparisons.json",
        [verdict.model_dump(mode="json") for verdict in comparisons],
    )
    return result
