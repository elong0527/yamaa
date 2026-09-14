"""Run one committed example through the engine and report what it observed.

#101 owns the conformance runner's invocation, report, and comparison
protocol, and #200 owns the R engine. This module owns the Python side of
that boundary: it runs an example through the same engine `yamaa_domain`
exposes, records what the run actually produced, and separately compares
those observations with the artifacts the example committed.

The two halves stay apart on purpose. `execute_example` opens a
specification, the sources it declares, and nothing else, so a run cannot
answer with the value it was supposed to produce. `compare_example` is the
only half that reads `expected/`, and it reads a finished report rather
than a live engine.

`REPORT_VERSION` carries a `-draft` suffix because #101 has not published
the serialization yet. The observations below are the ones #101's
requirements enumerate -- column order, row order, missing values, runtime
types, rendered values, `phase`, `condition`, `spec_paths`, declared
context, and R008-21 handler counts -- so ratifying that contract renames
this envelope rather than changing what the engine is asked for.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from collections.abc import Mapping, Sequence
from pathlib import Path
from typing import Literal, TypeAlias

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa import __version__
from yamaa.io import (
    Artifact,
    ArtifactTarget,
    ProjectResources,
    approve_roots,
    publish_artifact,
    render_artifact,
)
from yamaa.planning import ExecutionDiagnostic, execute_workflow, plan_workflow
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionSuccess,
    ExecutionUnsupported,
)
from yamaa.specification import SpecificationError, ValidationDiagnostic
from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.schema import load_schema_bundle

# Bumped when the envelope changes shape. The `-draft` suffix states that
# #101 has not ratified this serialization; a consumer that pins an exact
# version therefore fails loudly instead of reading a renamed field.
REPORT_VERSION = "0.1.0-draft"

RUNTIME: Literal["python"] = "python"

Outcome: TypeAlias = Literal["success", "failure", "unsupported", "error"]

SPEC_NAME = "spec.yaml"
EXPECTED_DIR = "expected"
ERROR_CONTRACT = "error.yaml"


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class ArtifactObservation(_FrozenModel):
    """One published artifact exactly as the run rendered it."""

    name: str = Field(min_length=1)
    profile: Literal["csv", "parquet"]
    columns: tuple[str, ...]
    types: tuple[str, ...]
    row_count: int = Field(ge=0)
    # R020's complete bytes split on the U+000A terminator R020-9 writes.
    # A missing value and a quoted empty string render differently and are
    # kept apart here; `sha256` below decides equality so that a newline
    # inside a quoted field cannot make this split the deciding view.
    records: tuple[str, ...]
    sha256: str = Field(min_length=64, max_length=64)


class DiagnosticObservation(_FrozenModel):
    """One structured failure in the shape the committed contracts use."""

    phase: str = Field(min_length=1)
    condition: str = Field(min_length=1)
    spec_paths: tuple[str, ...] = Field(min_length=1)
    requirement: str | None
    context: dict[str, JsonValue]


class UnsupportedObservation(_FrozenModel):
    """One valid declaration this engine does not execute yet."""

    operation: str = Field(min_length=1)
    spec_path: str = Field(min_length=1)


class HandlerObservation(_FrozenModel):
    """How often one declared handler path fired, zero included (R008-21)."""

    spec_path: str = Field(min_length=1)
    handler: str = Field(min_length=1)
    count: int = Field(ge=0)


class ExampleReport(_FrozenModel):
    """What one runtime observed running one example, and nothing more."""

    report_version: str = REPORT_VERSION
    runtime: Literal["python"] = RUNTIME
    runtime_version: str
    example: str = Field(min_length=1)
    outcome: Outcome
    artifacts: tuple[ArtifactObservation, ...] = ()
    diagnostics: tuple[DiagnosticObservation, ...] = ()
    unsupported: tuple[UnsupportedObservation, ...] = ()
    handler_counts: tuple[HandlerObservation, ...] = ()
    # Held for whoever reads a broken run. #101 keeps implementation error
    # text out of the portable comparison, so `compare_example` reads the
    # `error` outcome and never this string.
    error: str | None = None


class ComparisonFinding(_FrozenModel):
    """One difference between what an example committed and what ran."""

    kind: str = Field(min_length=1)
    detail: str = Field(min_length=1)
    expected: JsonValue = None
    actual: JsonValue = None


class ComparisonVerdict(_FrozenModel):
    """Whether one report satisfies the artifacts its example committed."""

    example: str = Field(min_length=1)
    runtime: Literal["python"] = RUNTIME
    kind: Literal["positive", "negative"]
    passed: bool
    findings: tuple[ComparisonFinding, ...] = ()


class ConformanceError(ValueError):
    """Raised when an example or its output location cannot be used."""


def _sha256(payload: bytes) -> str:
    return hashlib.sha256(payload).hexdigest()


def _records(payload: bytes) -> tuple[str, ...]:
    """Split R020's bytes on the U+000A terminator it writes after each."""
    text = payload.decode("utf-8")
    text = text.removesuffix("\n")
    return tuple(text.split("\n")) if text else ()


def _sorted_context(context: Mapping[str, JsonValue]) -> dict[str, JsonValue]:
    """Order context keys so one run's report is byte-identical to the next."""
    return {key: context[key] for key in sorted(context)}


def _observe_artifact(
    name: str, artifact: Artifact, target: Path
) -> ArtifactObservation:
    """Publish one artifact to a permitted target and record what it became."""
    payload = render_artifact(artifact)
    publish_artifact(ArtifactTarget(target), artifact)
    return ArtifactObservation(
        name=name,
        profile=artifact.profile,
        columns=tuple(column.name for column in artifact.columns),
        types=tuple(column.type for column in artifact.columns),
        row_count=artifact.frame.height,
        records=_records(payload) if artifact.profile == "csv" else (),
        sha256=_sha256(payload),
    )


def _observe_diagnostic(
    diagnostic: ValidationDiagnostic | ExecutionDiagnostic,
) -> DiagnosticObservation:
    return DiagnosticObservation(
        phase=diagnostic.phase,
        condition=diagnostic.condition,
        spec_paths=tuple(diagnostic.spec_paths),
        requirement=diagnostic.requirement,
        context=_sorted_context(diagnostic.context),
    )


def _isolated_destination(output_dir: Path, example: Path) -> Path:
    """Refuse an output location inside the example it would overwrite."""
    destination = output_dir.resolve()
    if destination == example or example in destination.parents:
        raise ConformanceError(
            "a conformance run writes outside the example directory; "
            f"{destination} sits inside {example}"
        )
    return destination


def _report(name: str, outcome: Outcome, **observations: object) -> ExampleReport:
    return ExampleReport(
        runtime_version=__version__,
        example=name,
        outcome=outcome,
        **observations,
    )


def _execute(
    name: str,
    entry: Path,
    schema_root: Path,
    destination: Path,
) -> ExampleReport:
    approved = approve_roots(entry)
    resources = ProjectResources(
        approved.project_root,
        base_directory=entry.parent,
        data_roots=approved.data_roots,
    )
    try:
        workflow = plan_workflow(entry, load_schema_bundle(schema_root), resources)
    except SpecificationError as error:
        # Validation fails before any source is read, so there is no handler
        # activity to report alongside it.
        return _report(
            name,
            "failure",
            diagnostics=tuple(_observe_diagnostic(item) for item in error.diagnostics),
        )

    execution = execute_workflow(workflow, resources)
    result = execution.result
    handler_counts = tuple(
        HandlerObservation(
            spec_path=count.spec_path,
            handler=count.handler,
            count=count.count,
        )
        for count in result.handler_counts
    )

    if isinstance(result, ExecutionFailure):
        return _report(
            name,
            "failure",
            diagnostics=tuple(_observe_diagnostic(item) for item in result.diagnostics),
            handler_counts=handler_counts,
        )
    if isinstance(result, ExecutionUnsupported):
        return _report(
            name,
            "unsupported",
            unsupported=tuple(
                UnsupportedObservation(
                    operation=feature.operation,
                    spec_path=feature.spec_path,
                )
                for feature in result.features
            ),
            handler_counts=handler_counts,
        )

    assert isinstance(result, ExecutionSuccess)
    entry_node = next(
        node for node in workflow.nodes if node.entry_path == workflow.entry_path
    )
    output = entry_node.resolved.specification.output
    published = [
        _observe_artifact(
            Path(output.path).stem,
            result.artifact,
            destination / Path(output.path).name,
        )
    ]
    if result.violation_log is not None and output.violation_log is not None:
        published.append(
            _observe_artifact(
                Path(output.violation_log).stem,
                result.violation_log,
                destination / Path(output.violation_log).name,
            )
        )
    return _report(
        name,
        "success",
        artifacts=tuple(published),
        handler_counts=handler_counts,
    )


def execute_example(
    example: str | Path,
    *,
    schema_root: str | Path,
    output_dir: str | Path,
) -> ExampleReport:
    """Run one example and report what the engine observed doing it.

    Only the specification and the sources it declares are opened. No
    `expected/` artifact is read here, so a report states what the engine
    produced rather than what it was supposed to produce; `compare_example`
    is the half that knows the committed answer.

    Artifacts are published under `output_dir`, which must sit outside the
    example, so a run cannot overwrite the fixture it is being judged
    against.
    """
    example_path = Path(example).resolve()
    entry = example_path / SPEC_NAME
    if not entry.is_file():
        raise ConformanceError(f"example has no {SPEC_NAME}: {example_path}")

    destination = _isolated_destination(Path(output_dir), example_path)
    destination.mkdir(parents=True, exist_ok=True)
    try:
        return _execute(
            example_path.name, entry, Path(schema_root).resolve(), destination
        )
    except ConformanceError:
        raise
    except Exception as failure:  # noqa: BLE001 - an engine crash is a result
        # An infrastructure failure is visible rather than silent, and it is
        # never a semantic failure: `compare_example` fails every example
        # that lands here. The text is for a human; nothing compares it.
        return _report(
            example_path.name,
            "error",
            error=f"{type(failure).__name__}: {failure}",
        )


def _finding(
    kind: str, detail: str, expected: object, actual: object
) -> ComparisonFinding:
    return ComparisonFinding(
        kind=kind,
        detail=detail,
        expected=expected,  # type: ignore[arg-type]
        actual=actual,  # type: ignore[arg-type]
    )


def expected_kind(example: str | Path) -> Literal["positive", "negative"]:
    """Say which committed artifact an example is judged against."""
    contract = Path(example) / EXPECTED_DIR / ERROR_CONTRACT
    return "negative" if contract.is_file() else "positive"


def _artifact_findings(
    observation: ArtifactObservation,
    payload: bytes,
) -> tuple[ComparisonFinding, ...]:
    """Compare one artifact byte for byte, then say where it first differs."""
    if observation.sha256 == _sha256(payload):
        return ()

    committed = _records(payload)
    produced = observation.records
    findings: list[ComparisonFinding] = []
    if committed and produced and committed[0] != produced[0]:
        findings.append(
            _finding(
                "artifact.columns",
                f"{observation.name}: header record differs",
                committed[0],
                produced[0],
            )
        )
    if len(committed) != len(produced):
        findings.append(
            _finding(
                "artifact.row_count",
                f"{observation.name}: record count differs",
                max(len(committed) - 1, 0),
                max(len(produced) - 1, 0),
            )
        )
    for index, (want, got) in enumerate(zip(committed[1:], produced[1:]), start=1):
        if want != got:
            findings.append(
                _finding(
                    "artifact.record",
                    f"{observation.name}: record {index} differs",
                    want,
                    got,
                )
            )
    if not findings:
        # The records agree yet the bytes do not: a terminator, an encoding,
        # or a newline inside a quoted field. The bytes decide.
        findings.append(
            _finding(
                "artifact.bytes",
                f"{observation.name}: rendered bytes differ",
                _sha256(payload),
                observation.sha256,
            )
        )
    return tuple(findings)


def _positive_findings(
    report: ExampleReport,
    example: Path,
) -> tuple[ComparisonFinding, ...]:
    if report.outcome != "success":
        return (
            _finding(
                "outcome",
                f"{report.example}: a positive example must execute",
                "success",
                report.outcome,
            ),
        )

    committed = {
        path.stem: path for path in sorted((example / EXPECTED_DIR).glob("*.csv"))
    }
    produced = {item.name: item for item in report.artifacts}
    findings: list[ComparisonFinding] = []
    for name in sorted(set(committed) | set(produced)):
        if name not in produced:
            findings.append(
                _finding("artifact.missing", f"{name}: not produced", name, None)
            )
        elif name not in committed:
            findings.append(
                _finding("artifact.unexpected", f"{name}: not committed", None, name)
            )
        else:
            findings.extend(
                _artifact_findings(produced[name], committed[name].read_bytes())
            )
    return tuple(findings)


def _negative_findings(
    report: ExampleReport,
    example: Path,
) -> tuple[ComparisonFinding, ...]:
    contract = read_yaml_document(example / EXPECTED_DIR / ERROR_CONTRACT)
    if not isinstance(contract, dict):
        raise ConformanceError(f"{example.name}: {ERROR_CONTRACT} is not a mapping")

    # Unsupported execution and an engine crash are not the semantic failure
    # a negative example commits, so neither one passes here.
    if report.outcome != "failure":
        return (
            _finding(
                "outcome",
                f"{report.example}: a negative example must fail semantically",
                "failure",
                report.outcome,
            ),
        )

    observed = report.diagnostics[0]
    findings: list[ComparisonFinding] = []
    for field in ("phase", "condition"):
        want = contract[field]
        got = getattr(observed, field)
        if want != got:
            findings.append(
                _finding(f"diagnostic.{field}", f"{report.example}: {field}", want, got)
            )

    paths = tuple(contract["spec_paths"])
    if paths != observed.spec_paths:
        findings.append(
            _finding(
                "diagnostic.spec_paths",
                f"{report.example}: spec_paths",
                list(paths),
                list(observed.spec_paths),
            )
        )

    if "requirement" in contract and contract["requirement"] != observed.requirement:
        findings.append(
            _finding(
                "diagnostic.requirement",
                f"{report.example}: requirement",
                contract["requirement"],
                observed.requirement,
            )
        )

    # The contract states the context a portable failure must carry. A
    # runtime may observe more; it may not observe a different value.
    for key in sorted(contract.get("context") or {}):
        want = contract["context"][key]
        if key not in observed.context:
            findings.append(
                _finding(
                    "diagnostic.context",
                    f"{report.example}: context.{key} absent",
                    want,
                    None,
                )
            )
        elif observed.context[key] != want:
            findings.append(
                _finding(
                    "diagnostic.context",
                    f"{report.example}: context.{key}",
                    want,
                    observed.context[key],
                )
            )
    return tuple(findings)


def _handler_findings(
    report: ExampleReport,
    expected: Sequence[HandlerObservation],
) -> tuple[ComparisonFinding, ...]:
    produced = {
        (item.spec_path, item.handler): item.count for item in report.handler_counts
    }
    committed = {(item.spec_path, item.handler): item.count for item in expected}
    findings: list[ComparisonFinding] = []
    for path, handler in sorted(set(committed) | set(produced)):
        key = (path, handler)
        detail = f"{report.example}: {path} {handler}"
        if key not in produced:
            findings.append(
                _finding(
                    "handler.missing", f"{detail} not reported", committed[key], None
                )
            )
        elif key not in committed:
            findings.append(
                _finding(
                    "handler.unexpected", f"{detail} not committed", None, produced[key]
                )
            )
        elif committed[key] != produced[key]:
            findings.append(
                _finding("handler.count", detail, committed[key], produced[key])
            )
    return tuple(findings)


def compare_example(
    report: ExampleReport,
    example: str | Path,
    *,
    expected_handler_counts: Sequence[HandlerObservation] | None = None,
) -> ComparisonVerdict:
    """Judge one finished report against the artifacts its example committed.

    This is the only half that reads `expected/`, and it reads a report
    rather than an engine. Nothing here normalizes: column order, record
    order, a missing value, and a quoted empty string are compared as
    rendered, and the artifact verdict is the complete bytes.

    R008-21 requires a run to report every declared handler path. The counts
    themselves become a comparison only when a caller states them, because
    no example commits them yet; #101's cross-runtime comparator is where R
    and Python counts meet.
    """
    example_path = Path(example).resolve()
    kind = expected_kind(example_path)
    findings = (
        _negative_findings(report, example_path)
        if kind == "negative"
        else _positive_findings(report, example_path)
    )
    if expected_handler_counts is not None:
        findings = findings + _handler_findings(report, expected_handler_counts)
    return ComparisonVerdict(
        example=report.example,
        kind=kind,
        passed=not findings,
        findings=findings,
    )


def write_report(report: ExampleReport, report_dir: Path) -> Path:
    """Write one report where a runner collects it, deterministically."""
    report_dir.mkdir(parents=True, exist_ok=True)
    path = report_dir / f"{report.example}.{RUNTIME}.json"
    payload = json.dumps(report.model_dump(mode="json"), indent=2)
    path.write_text(f"{payload}\n", encoding="utf-8")
    return path


def _describe(verdict: ComparisonVerdict | None) -> str:
    if verdict is None:
        return "reported"
    return "pass" if verdict.passed else "FAIL"


def main(argv: Sequence[str] | None = None) -> int:
    """Execute the named examples, write a report each, and judge them."""
    parser = argparse.ArgumentParser(
        prog="python -m yamaa.adapters.conformance",
        description=(
            "Execute committed examples through the Python engine, write one "
            "report each, and compare them with the committed artifacts."
        ),
    )
    parser.add_argument("examples", nargs="+", help="example directory names")
    parser.add_argument(
        "--run-dir",
        required=True,
        type=Path,
        help="where artifacts and reports are written; outside the examples",
    )
    parser.add_argument(
        "--examples-root",
        type=Path,
        default=Path("yaml/examples"),
        help="directory holding the committed examples",
    )
    parser.add_argument(
        "--schema-root",
        type=Path,
        default=None,
        help="directory holding schema.yaml; the examples root's parent by default",
    )
    parser.add_argument(
        "--no-compare",
        action="store_true",
        help="write reports without judging them against expected/",
    )
    args = parser.parse_args(argv)

    examples_root = args.examples_root.resolve()
    schema_root = (
        args.schema_root.resolve() if args.schema_root else examples_root.parent
    )
    report_dir = args.run_dir / "reports"
    failed = False

    for name in args.examples:
        example = examples_root / name
        report = execute_example(
            example,
            schema_root=schema_root,
            output_dir=args.run_dir / "artifacts" / name,
        )
        path = write_report(report, report_dir)
        verdict = None if args.no_compare else compare_example(report, example)
        if verdict is not None and not verdict.passed:
            failed = True
        if report.outcome == "error":
            failed = True
        print(f"{name}  {RUNTIME}  {report.outcome}  {_describe(verdict)}  {path}")
        if report.error is not None:
            print(f"    error: {report.error}")
        for finding in verdict.findings if verdict else ():
            print(f"    {finding.kind}: {finding.detail}")
            print(f"      committed: {finding.expected!r}")
            print(f"      produced:  {finding.actual!r}")

    return 1 if failed else 0


if __name__ == "__main__":  # pragma: no cover - module entry point
    sys.exit(main())
