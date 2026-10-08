"""Reconcile complete public-domain runs against the independent M1 protocol.

This comparator executes no candidate engine. Installed public-entrypoint tests
retain actual Rust reports after comparing all original run observations and
saved bytes. The broad reference-assisted inventory remains separate.
"""

from __future__ import annotations

import argparse
import json
import shutil
from pathlib import Path

from yamaa.adapters.qualification import Batch, MissingRoute, load_inventory, qualify

COHORT = (
    "adam-adlb-ordered-sum",
    "schema-lookup",
    "schema-window-functions",
    "schema-inheritance",
    "negative-zero-division",
    "negative-integer-overflow",
)


def read(path):
    return json.loads(path.read_text(encoding="utf-8"))


def installed_evidence(directory, runtime, source_revision):
    supplemental = directory / "supplemental" if runtime == "python" else directory
    record = read(supplemental / "supplemental.json")
    evidence = record["evidence"]
    if evidence["source_revision"] != source_revision:
        raise ValueError("stale installed public-run evidence")
    suites = {s["id"]: s for s in record["suites"]}
    suite = suites[runtime + "/original-specifications"]
    if suite["result"] != "pass" or suite["exit_code"] != 0:
        raise ValueError("public original-document suite did not pass")
    log = (supplemental / suite["log"]).read_text()
    if runtime == "python" and "skipped=" in log:
        raise ValueError("skipped public file runs cannot qualify the cohort")
    if runtime == "r":
        for name in COHORT:
            witness = (
                name
                + " public domain complete Rust run report and successful save observations passed"
            )
            if witness not in log:
                raise ValueError("missing public R complete-run witness: " + name)
    installation = record["installation"]
    return record, Batch(
        runtime=runtime,
        backend="rust",
        level="shared_run",
        source_revision=source_revision,
        host_package_version=installation["host_package_version"],
        core_version=installation["core_version"],
        binding_package_version=installation.get("binding_package_version"),
        artifact_reference=evidence["artifact_reference"],
        evidence=evidence["run"],
        reports_dir=str((directory / "public-reports").resolve()),
    )


def retain_reports(batch, source, destination):
    destination.mkdir()
    for name in COHORT:
        filename = f"{name}.{batch.runtime}.{batch.backend}.json"
        shutil.copyfile(source / filename, destination / filename)
    return batch.model_copy(update={"reports_dir": str(destination.resolve())})


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fixtures", type=Path, required=True)
    parser.add_argument("--python-evidence", type=Path, required=True)
    parser.add_argument("--r-evidence", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--source-revision", required=True)
    args = parser.parse_args(argv)
    if args.output.exists():
        parser.error("output must be a fresh evidence directory")
    if args.output.resolve().is_relative_to(args.fixtures.resolve()):
        parser.error("output must be outside the committed fixture tree")
    manifest = load_inventory(args.fixtures)
    python_record, python_batch = installed_evidence(
        args.python_evidence, "python", args.source_revision
    )
    r_batch = None
    if args.r_evidence is not None:
        r_record, r_batch = installed_evidence(
            args.r_evidence, "r", args.source_revision
        )
        # Runner kernel/image patch versions may differ between independent jobs.
        if (
            python_record["platform"].split("-", 1)[0]
            != r_record["platform"].split("-", 1)[0]
        ):
            raise ValueError(
                "Python and R complete-run evidence must share an OS family"
            )
        if python_batch.core_version != r_batch.core_version:
            raise ValueError(
                "Python and R complete-run evidence must share a core version"
            )
        if python_batch.evidence != r_batch.evidence:
            raise ValueError(
                "Python and R complete-run evidence must share an Actions run"
            )
    elif not python_record["platform"].lower().startswith("windows-"):
        raise ValueError("Unix cohort qualification requires both installed hosts")
    reference = Batch.model_validate_json(
        (args.python_evidence / "python-batch.json").read_text()
    )
    if (
        reference.runtime != "python"
        or reference.backend != "python"
        or reference.level != "reference_run"
    ):
        raise ValueError("independent reference batch is required")
    if reference.source_revision != args.source_revision:
        raise ValueError("stale independent reference batch")
    args.output.mkdir(parents=True)
    evidence_directories = [("python", args.python_evidence)]
    if args.r_evidence is not None:
        evidence_directories.append(("r", args.r_evidence))
    for runtime, directory in evidence_directories:
        supplemental = directory / "supplemental" if runtime == "python" else directory
        shutil.copyfile(
            supplemental / "supplemental.json",
            args.output / (runtime + "-installed-evidence.json"),
        )
    fixtures = args.output / "fixtures"
    fixtures.mkdir()
    for name in COHORT:
        shutil.copytree(args.fixtures / name, fixtures / name, symlinks=True)
    (fixtures / "execution-manifest.yaml").write_text(
        json.dumps(
            {
                "version": manifest.version,
                "examples": {
                    name: manifest.examples[name].model_dump(mode="json")
                    for name in COHORT
                },
            }
        ),
        encoding="utf-8",
    )
    batches = [
        retain_reports(
            reference,
            args.python_evidence / "reports" / "python",
            args.output / "reference-reports",
        ),
        retain_reports(
            python_batch,
            args.python_evidence / "public-reports",
            args.output / "python-reports",
        ),
    ]
    if r_batch is not None:
        batches.append(
            retain_reports(
                r_batch, args.r_evidence / "public-reports", args.output / "r-reports"
            )
        )
    required_routes = [
        ("python", "python", "reference_run"),
        ("python", "rust", "shared_run"),
    ]
    if r_batch is not None:
        required_routes.append(("r", "rust", "shared_run"))
    required = tuple(
        (name, runtime, backend, level)
        for name in COHORT
        for runtime, backend, level in required_routes
    )
    missing_routes = (
        ()
        if r_batch is not None
        else (
            MissingRoute(
                runtime="r",
                backend="rust",
                blocker="windows_r_scope_unqualified",
                issue="#1742",
            ),
        )
    )
    inventory = qualify(
        fixtures,
        args.source_revision,
        tuple(batches),
        required=required,
        missing_routes=missing_routes,
    )
    (args.output / "coverage.json").write_text(
        inventory.model_dump_json(indent=2) + "\n", encoding="utf-8"
    )
    print(json.dumps(inventory.summary, indent=2))
    for error in inventory.errors:
        print(error)
    return int(bool(inventory.errors))


if __name__ == "__main__":
    raise SystemExit(main())
