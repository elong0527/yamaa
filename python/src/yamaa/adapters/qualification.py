"""Reconcile benchmark coverage using the existing conformance comparators.

This reporter executes no engines. A batch describes installed run evidence;
missing reports remain visible, and reference-assisted native runs never imply
shared compilation. Supplemental component probes are outside this YAML inventory.
"""

from __future__ import annotations

import argparse
import json
from collections import Counter
from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, TypeAdapter, model_validator

from yamaa.adapters.conformance import (
    ComparisonFinding,
    compare_example,
    compare_reports,
    read_report,
)
from yamaa.specification._yaml import read_yaml_document

Host = Literal["python", "r"]
Backend = Literal["python", "rust"]
Level = Literal["reference_run", "reference_assisted_run", "shared_run"]
Result = Literal[
    "pass",
    "semantic_mismatch",
    "unsupported",
    "not_exercised",
    "infrastructure_failure",
]
TARGETS = (("python", "python"), ("python", "rust"), ("r", "rust"))


class Record(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class ManifestEntry(Record):
    """Keep the reference manifest's declarations independently of observed results."""

    status: Literal["executable", "blocked"]
    runtimes: list[Host] = Field(default_factory=list)
    blocked_by: str | None = Field(default=None, pattern=r"^#[1-9][0-9]*$")

    @model_validator(mode="after")
    def validate_status(self):
        if self.status == "executable" and not self.runtimes:
            raise ValueError("executable entries must declare runtimes")
        if self.status == "blocked" and self.blocked_by is None:
            raise ValueError("blocked entries must name an issue")
        return self


class Manifest(Record):
    version: Literal["1.0"]
    examples: dict[str, ManifestEntry] = Field(min_length=1)


class Batch(Record):
    """Provenance of reports from one installed host/backend and one revision."""

    runtime: Host
    backend: Backend
    level: Level
    source_revision: str = Field(min_length=1)
    host_package_version: str = Field(min_length=1)
    core_version: str | None = Field(default=None, min_length=1)
    artifact_reference: str = Field(min_length=1)
    evidence: str = Field(min_length=1)
    reports_dir: str = Field(min_length=1)

    @model_validator(mode="after")
    def validate_route(self):
        if (self.runtime, self.backend) not in TARGETS:
            raise ValueError("unknown host/backend route")
        if (self.backend == "python") != (self.level == "reference_run"):
            raise ValueError("reference_run requires the reference Python backend")
        if self.backend == "rust" and self.core_version is None:
            raise ValueError("Rust evidence requires its core version")
        if self.runtime == "r" and self.level == "reference_assisted_run":
            raise ValueError("R qualification cannot use the temporary Python planner")
        return self


class Coverage(Record):
    example: str
    family: str
    runtime: Host
    backend: Backend
    level: Level | None
    result: Result
    blockers: tuple[str, ...] = ()
    report: str | None = None
    findings: tuple[ComparisonFinding, ...] = ()
    error: str | None = None


class Inventory(Record):
    version: Literal["1.0"] = "1.0"
    source_revision: str
    manifest: Manifest
    batches: tuple[Batch, ...]
    coverage: tuple[Coverage, ...]
    errors: tuple[str, ...]
    summary: dict[str, dict[str, int]]


def load_inventory(examples_root: Path) -> Manifest:
    """Reject stale/missing fixtures, duplicate YAML keys and malformed declarations."""
    manifest = Manifest.model_validate(
        read_yaml_document(examples_root / "execution-manifest.yaml")
    )
    actual = {
        path.name
        for path in examples_root.iterdir()
        if path.is_dir() and any(path.glob("spec*.yaml"))
    }
    declared = set(manifest.examples)
    if actual != declared:
        raise ValueError(
            f"fixture inventory differs: missing entries={sorted(actual - declared)}, "
            f"stale entries={sorted(declared - actual)}"
        )
    return manifest


def qualify(
    examples_root: Path,
    source_revision: str,
    batches: tuple[Batch, ...],
    *,
    required: tuple[tuple[str, Host, Backend, Level], ...] = (),
) -> Inventory:
    """Judge reports and enforce explicitly qualified routes without hiding gaps.

    Native success needs both independent expected artifacts and portable parity
    with a passing reference report from the same revision. Required tuples pin
    the qualification level, so a Python-assisted pass cannot satisfy shared_run.
    """
    if not source_revision:
        raise ValueError("source revision is required")
    manifest = load_inventory(examples_root)
    routes = {}
    reports = {}
    broken = {}
    errors = []
    for batch in batches:
        key = batch.runtime, batch.backend
        if key in routes:
            raise ValueError(f"duplicate batch: {key}")
        if batch.source_revision != source_revision:
            raise ValueError(f"stale batch revision: {key}")
        routes[key] = batch
        directory = Path(batch.reports_dir)
        if not directory.is_dir():
            errors.append(f"missing report directory: {directory}")
            for name in manifest.examples:
                broken[(name, *key)] = "report directory is unavailable"
            continue
        for path in sorted(directory.glob("*.json")):
            # Dedicated report directories contain only ExampleReport envelopes.
            try:
                report = read_report(path)
                if report.outcome == "failure" and not report.diagnostics:
                    raise ValueError("failure report requires at least one diagnostic")
                identity = report.example, report.runtime, report.backend
                expected_name = ".".join(identity) + ".json"
                if path.name != expected_name or identity[1:] != key:
                    raise ValueError(
                        "report filename or route does not match its batch"
                    )
                if report.example not in manifest.examples:
                    raise ValueError("report names a fixture absent from the inventory")
                engine_version = (
                    batch.core_version
                    if batch.backend == "rust"
                    else batch.host_package_version
                )
                if report.engine_version != engine_version:
                    raise ValueError("report engine version does not match its batch")
                reports[identity] = (report, path)
            except (OSError, ValueError) as exc:
                errors.append(f"invalid report {path}: {exc}")
                # Associate a malformed envelope with its expected fixture when possible.
                suffix = f".{batch.runtime}.{batch.backend}.json"
                if path.name.endswith(suffix):
                    broken[(path.name.removesuffix(suffix), *key)] = str(exc)

    coverage = []
    for name in sorted(manifest.examples):
        for runtime, backend in TARGETS:
            key = name, runtime, backend
            batch = routes.get((runtime, backend))
            base = {
                "example": name,
                "family": name.split("-", 1)[0],
                "runtime": runtime,
                "backend": backend,
                "level": None if batch is None else batch.level,
            }
            if key in broken:
                item = Coverage(
                    **base, result="infrastructure_failure", error=broken[key]
                )
            elif key not in reports:
                item = Coverage(
                    **base, result="not_exercised", blockers=("report_not_supplied",)
                )
            else:
                report, path = reports[key]
                base["report"] = str(path)
                if report.outcome == "error":
                    item = Coverage(
                        **base, result="infrastructure_failure", error=report.error
                    )
                elif report.outcome == "unsupported":
                    item = Coverage(
                        **base,
                        result="unsupported",
                        blockers=tuple(
                            sorted(
                                {
                                    f"unsupported:{u.operation}"
                                    for u in report.unsupported
                                }
                            )
                        ),
                    )
                else:
                    try:
                        findings = list(
                            compare_example(report, examples_root / name).findings
                        )
                        reference = reports.get((name, "python", "python"))
                        if backend == "rust" and (
                            reference is None
                            or not compare_example(
                                reference[0], examples_root / name
                            ).passed
                        ):
                            item = Coverage(
                                **base,
                                result="infrastructure_failure",
                                blockers=("passing_reference_required",),
                                findings=tuple(findings),
                            )
                        else:
                            if backend == "rust":
                                findings.extend(
                                    compare_reports(reference[0], report).findings
                                )
                            item = Coverage(
                                **base,
                                result="semantic_mismatch" if findings else "pass",
                                findings=tuple(findings),
                            )
                    except (OSError, ValueError) as exc:
                        item = Coverage(
                            **base, result="infrastructure_failure", error=str(exc)
                        )
            coverage.append(item)

    index = {(r.example, r.runtime, r.backend, r.level): r for r in coverage}
    for identity in required:
        row = index.get(identity)
        if row is None or row.result != "pass":
            errors.append(f"required qualification missing or regressed: {identity}")
    # The existing manifest is an independent baseline, never rewritten from runs.
    for name, entry in manifest.examples.items():
        if entry.status == "executable" and "python" in entry.runtimes:
            row = index.get((name, "python", "python", "reference_run"))
            if row is None or row.result != "pass":
                errors.append(f"reference baseline missing or regressed: {name}")
    for row in coverage:
        if row.result in {"semantic_mismatch", "infrastructure_failure"}:
            errors.append(f"{row.example}/{row.runtime}/{row.backend}: {row.result}")
    counts = {}
    for row in coverage:
        key = f"{row.runtime}/{row.backend}/{row.level or 'unqualified'}/{row.family}"
        counts.setdefault(key, Counter())[row.result] += 1
    return Inventory(
        source_revision=source_revision,
        manifest=manifest,
        batches=batches,
        coverage=tuple(coverage),
        errors=tuple(errors),
        summary={
            key: dict(sorted(value.items())) for key, value in sorted(counts.items())
        },
    )


def main(argv=None) -> int:
    """Combine installed report batches without running engines or changing goldens."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--examples-root", type=Path, default=Path("benchmarks"))
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--batch", type=Path, action="append", default=[])
    parser.add_argument(
        "--required",
        type=Path,
        help="JSON array of [fixture, host, backend, level] gates",
    )
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args(argv)
    if args.output.resolve().is_relative_to(args.examples_root.resolve()):
        parser.error("inventory output must be outside the benchmark tree")
    batches = tuple(Batch.model_validate_json(path.read_text()) for path in args.batch)
    required = (
        ()
        if args.required is None
        else TypeAdapter(tuple[tuple[str, Host, Backend, Level], ...]).validate_json(
            args.required.read_text(), strict=True
        )
    )
    inventory = qualify(
        args.examples_root, args.source_revision, batches, required=required
    )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(inventory.model_dump_json(indent=2) + "\n", encoding="utf-8")
    print(json.dumps(inventory.summary, indent=2))
    for error in inventory.errors:
        print(error)
    return int(bool(inventory.errors))


if __name__ == "__main__":
    raise SystemExit(main())
