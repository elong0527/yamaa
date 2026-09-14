"""Reference comparator for the versioned conformance contract."""

from __future__ import annotations

import hashlib
import json
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

from pydantic import ValidationError

import yaml
from conformance.models import (
    ArtifactObservation,
    ComparisonSummary,
    Diagnostic,
    FailureOutcome,
    HandlerCount,
    Invocation,
    Report,
    SuccessOutcome,
)


@dataclass(frozen=True, slots=True)
class _Run:
    label: str
    root: Path
    invocation_path: Path
    invocation: Invocation
    report_path: Path
    report: Report
    resolved_data: object | None


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _load_model(path: Path, model: type[Invocation | Report]):
    return model.model_validate_json(path.read_text(encoding="utf-8"), strict=True)


def _contained(root: Path, relative: str) -> Path:
    root = root.resolve()
    candidate = (root / relative).resolve()
    try:
        candidate.relative_to(root)
    except ValueError as error:
        raise ValueError(f"path escapes runtime directory: {relative}") from error
    return candidate


def _load_runtime(run_root: Path, label: str, failures: list[str]) -> _Run | None:
    runtime_root = (run_root / label).resolve()
    invocation_path = runtime_root / "invocation.json"
    report_path = runtime_root / "report.json"
    try:
        invocation = _load_model(invocation_path, Invocation)
        report = _load_model(report_path, Report)
    except (OSError, ValueError, ValidationError) as error:
        failures.append(f"{label}: invalid or missing contract file: {error}")
        return None

    if Path(invocation.output_directory).resolve() != runtime_root:
        failures.append(
            f"{label}: output_directory does not match its isolated runtime directory"
        )
    if invocation.runtime != label:
        failures.append(
            f"{label}: invocation runtime is {invocation.runtime!r}, expected {label!r}"
        )
    if report.runtime.name != label:
        failures.append(
            f"{label}: report runtime is {report.runtime.name!r}, expected {label!r}"
        )
    if report.protocol_version != invocation.protocol_version:
        failures.append(f"{label}: invocation and report protocol versions differ")
    if report.example != invocation.example:
        failures.append(f"{label}: invocation and report examples differ")
    if report.invocation_sha256 != _sha256(invocation_path):
        failures.append(f"{label}: report does not identify its invocation bytes")

    resolved_data: object | None = None
    observation = report.specification
    if observation is not None:
        if observation.entrypoint != invocation.entrypoint:
            failures.append(f"{label}: report entrypoint differs from invocation")
        for document in observation.documents:
            path = _contained(Path(invocation.project_root), document.path)
            if not path.is_file() or _sha256(path) != document.sha256:
                failures.append(
                    f"{label}: specification digest does not match {document.path}"
                )
        if observation.resolved is not None:
            resolved_path = _contained(runtime_root, observation.resolved.path)
            if not resolved_path.is_file():
                failures.append(f"{label}: resolved specification is missing")
            elif _sha256(resolved_path) != observation.resolved.sha256:
                failures.append(f"{label}: resolved specification digest differs")
            else:
                try:
                    resolved_data = json.loads(
                        resolved_path.read_text(encoding="utf-8")
                    )
                except (OSError, json.JSONDecodeError) as error:
                    failures.append(
                        f"{label}: resolved specification is not JSON: {error}"
                    )
        if isinstance(report.outcome, SuccessOutcome):
            for artifact in report.outcome.artifacts:
                path = _contained(runtime_root, artifact.produced_path)
                if not path.is_file() or _sha256(path) != artifact.sha256:
                    failures.append(
                        f"{label}: artifact digest does not match "
                        f"{artifact.produced_path}"
                    )

    return _Run(
        label,
        runtime_root,
        invocation_path,
        invocation,
        report_path,
        report,
        resolved_data,
    )


def _common_invocation(invocation: Invocation) -> dict[str, object]:
    return invocation.model_dump(exclude={"runtime", "output_directory"})


def _normalized_sources(report: Report) -> tuple[object, ...]:
    return tuple(
        item.model_dump()
        for item in sorted(report.sources, key=lambda item: item.dataset)
    )


def _normalized_handlers(report: Report) -> tuple[object, ...]:
    return tuple(
        item.model_dump()
        for item in sorted(
            report.handler_counts,
            key=lambda item: (item.spec_path, item.handler),
        )
    )


def _expected_output_schema(resolved: object) -> tuple[tuple[str, str], ...]:
    if not isinstance(resolved, Mapping):
        raise TypeError("resolved specification root must be an object")
    output = resolved.get("output")
    columns = resolved.get("columns")
    if not isinstance(output, Mapping) or not isinstance(columns, list):
        raise TypeError("resolved specification lacks output or columns")
    selected = output.get("columns")
    if not isinstance(selected, list) or not all(
        isinstance(item, str) for item in selected
    ):
        raise ValueError("resolved output.columns must be a string array")
    declared: dict[str, str] = {}
    for column in columns:
        if not isinstance(column, Mapping):
            raise TypeError("resolved columns must contain objects")
        name = column.get("name")
        column_type = column.get("type")
        if not isinstance(name, str) or not isinstance(column_type, str):
            raise TypeError("resolved column requires string name and type")
        declared[name] = column_type
    try:
        return tuple((name, declared[name]) for name in selected)
    except KeyError as error:
        raise ValueError(
            f"resolved output names unknown column {error.args[0]}"
        ) from error


def _compare_provenance(runs: Sequence[_Run], failures: list[str]) -> None:
    first = runs[0]
    for run in runs[1:]:
        if _common_invocation(run.invocation) != _common_invocation(first.invocation):
            failures.append(
                f"{run.label}: case inputs differ from {first.label}'s invocation"
            )
        if run.report.specification != first.report.specification:
            # Resolved paths and digests are runtime-local, so compare their decoded
            # data and the authored document observations separately below.
            left = run.report.specification
            right = first.report.specification
            left_docs = None if left is None else (left.entrypoint, left.documents)
            right_docs = None if right is None else (right.entrypoint, right.documents)
            if left_docs != right_docs:
                failures.append(
                    f"{run.label}: specification inputs differ from {first.label}"
                )
        if run.resolved_data != first.resolved_data:
            failures.append(
                f"{run.label}: resolved specification differs from {first.label}"
            )
        if _normalized_sources(run.report) != _normalized_sources(first.report):
            failures.append(f"{run.label}: source snapshots differ from {first.label}")


def _context_contains(actual: object, expected: object) -> bool:
    if isinstance(expected, Mapping):
        return isinstance(actual, Mapping) and all(
            key in actual and _context_contains(actual[key], value)
            for key, value in expected.items()
        )
    if isinstance(expected, list):
        return (
            isinstance(actual, list)
            and len(actual) == len(expected)
            and all(
                _context_contains(a, e) for a, e in zip(actual, expected, strict=True)
            )
        )
    return actual == expected


def _compare_diagnostic(
    label: str,
    diagnostic: Diagnostic,
    expected: Mapping[str, object],
    failures: list[str],
) -> None:
    for field in ("phase", "condition", "spec_paths", "requirement"):
        if field in expected:
            actual = getattr(diagnostic, field)
            if isinstance(actual, tuple):
                actual = list(actual)
            if actual != expected[field]:
                failures.append(f"{label}: diagnostic {field} differs from expected")
    if not _context_contains(diagnostic.context, expected.get("context", {})):
        failures.append(f"{label}: diagnostic context lacks required expected values")


def _handler_contract_key(run: _Run, expected_root: Path) -> str:
    entrypoint = Path(run.invocation.project_root) / run.invocation.entrypoint
    return entrypoint.resolve().relative_to(expected_root.parent.resolve()).as_posix()


def _load_handler_contract(
    expected_root: Path,
    key: str,
    failures: list[str],
) -> tuple[dict[str, object], ...] | None:
    path = expected_root / "handler-counts.yaml"
    if not path.exists():
        return None
    try:
        document = yaml.safe_load(path.read_text(encoding="utf-8"))
        if not isinstance(document, Mapping) or document.get("version") != "1.0":
            raise ValueError('root requires version: "1.0"')
        specifications = document.get("specifications")
        if not isinstance(specifications, Mapping) or key not in specifications:
            raise ValueError(f"missing specification entry {key!r}")
        rows = specifications[key]
        if not isinstance(rows, list):
            raise TypeError(f"{key!r} must contain a list")
        validated = tuple(HandlerCount.model_validate(row) for row in rows)
    except (OSError, TypeError, ValueError, ValidationError, yaml.YAMLError) as error:
        failures.append(f"invalid handler-count contract: {error}")
        return ()
    return tuple(
        item.model_dump()
        for item in sorted(validated, key=lambda item: (item.spec_path, item.handler))
    )


def _compare_handlers(
    runs: Sequence[_Run], expected_root: Path, failures: list[str]
) -> None:
    first = runs[0]
    first_counts = _normalized_handlers(first.report)
    for run in runs[1:]:
        if _normalized_handlers(run.report) != first_counts:
            failures.append(f"{run.label}: handler counts differ from {first.label}")
    try:
        key = _handler_contract_key(first, expected_root)
    except ValueError:
        failures.append("entrypoint is not inside the selected example directory")
        return
    expected = _load_handler_contract(expected_root, key, failures)
    if expected is None:
        if first_counts:
            failures.append(
                "handler counts exist but expected/handler-counts.yaml does not"
            )
    elif first_counts != expected:
        failures.append("handler counts differ from expected/handler-counts.yaml")


def _compare_positive(
    runs: Sequence[_Run], expected_root: Path, failures: list[str]
) -> None:
    artifacts_by_run: list[tuple[_Run, dict[str, ArtifactObservation]]] = []
    for run in runs:
        if not isinstance(run.report.outcome, SuccessOutcome):
            failures.append(
                f"{run.label}: positive example returned "
                f"{run.report.outcome.status!r}, not 'success'"
            )
            continue
        artifacts = {
            artifact.role: artifact for artifact in run.report.outcome.artifacts
        }
        artifacts_by_run.append((run, artifacts))
        for artifact in artifacts.values():
            if artifact.profile != "csv":
                failures.append(
                    f"{run.label}: protocol 1.0 golden comparison requires csv "
                    f"for the {artifact.role} artifact"
                )
                continue
            expected = expected_root / Path(artifact.declared_path).name
            produced = _contained(run.root, artifact.produced_path)
            if not expected.is_file():
                failures.append(
                    f"{run.label}: expected artifact is missing: {expected.name}"
                )
            elif produced.is_file() and produced.read_bytes() != expected.read_bytes():
                failures.append(
                    f"{run.label}: artifact bytes differ from expected/{expected.name}"
                )

        primary = artifacts["primary"]
        if run.resolved_data is not None:
            try:
                expected_schema = _expected_output_schema(run.resolved_data)
            except (TypeError, ValueError) as error:
                failures.append(f"{run.label}: {error}")
            else:
                observed_schema = tuple(
                    (column.name, column.type) for column in primary.table.columns
                )
                if observed_schema != expected_schema:
                    failures.append(
                        f"{run.label}: artifact column names or runtime types "
                        "differ from the resolved specification"
                    )

    if artifacts_by_run:
        first_run, first = artifacts_by_run[0]
        expected_names = {path.name for path in expected_root.glob("*.csv")}
        declared_names = {
            Path(artifact.declared_path).name for artifact in first.values()
        }
        if declared_names != expected_names:
            failures.append(
                "reported artifacts do not match every committed csv golden"
            )
        for run, artifacts in artifacts_by_run[1:]:
            if set(artifacts) != set(first):
                failures.append(
                    f"{run.label}: artifact roles differ from {first_run.label}"
                )
                continue
            for role, artifact in artifacts.items():
                baseline = first[role]
                if (
                    artifact.declared_path,
                    artifact.profile,
                ) != (
                    baseline.declared_path,
                    baseline.profile,
                ):
                    failures.append(
                        f"{run.label}: {role} artifact identity differs from "
                        f"{first_run.label}"
                    )
                if artifact.table != baseline.table:
                    failures.append(
                        f"{run.label}: typed cells, missingness, order, or temporal "
                        f"precision differ from {first_run.label} for {role}"
                    )


def _compare_negative(
    runs: Sequence[_Run], expected_root: Path, failures: list[str]
) -> None:
    try:
        expected = yaml.safe_load(
            (expected_root / "error.yaml").read_text(encoding="utf-8")
        )
    except (OSError, yaml.YAMLError) as error:
        failures.append(f"cannot read expected/error.yaml: {error}")
        return
    if not isinstance(expected, Mapping):
        failures.append("expected/error.yaml must contain an object")
        return
    for run in runs:
        if not isinstance(run.report.outcome, FailureOutcome):
            failures.append(
                f"{run.label}: negative example returned "
                f"{run.report.outcome.status!r}, not 'failure'"
            )
            continue
        errors = [
            item for item in run.report.outcome.diagnostics if item.severity == "error"
        ]
        if len(errors) != 1:
            failures.append(
                f"{run.label}: negative example must report exactly one error"
            )
            continue
        _compare_diagnostic(run.label, errors[0], expected, failures)


def compare_case(
    run_root: str | Path,
    expected_root: str | Path,
    runtimes: Sequence[str] = ("r", "python"),
) -> ComparisonSummary:
    """Validate and compare one example's adapter runs and committed contract."""

    selected = tuple(runtimes)
    failures: list[str] = []
    if not selected or len(selected) != len(set(selected)):
        failures.append("runtimes must be a non-empty list without duplicates")
    runs = [
        run
        for label in selected
        if (run := _load_runtime(Path(run_root), label, failures)) is not None
    ]
    example = runs[0].invocation.example if runs else "unknown"
    checks = ("contract", "provenance", "golden", "runtime-parity")
    if len(runs) == len(selected) and runs:
        _compare_provenance(runs, failures)
        expected = Path(expected_root).resolve()
        required_expected = (
            Path(runs[0].invocation.project_root)
            / Path(runs[0].invocation.entrypoint).parent
            / "expected"
        ).resolve()
        if expected != required_expected:
            failures.append("expected_root does not belong to the invoked example")
        elif (expected / "error.yaml").is_file():
            _compare_negative(runs, expected, failures)
        else:
            _compare_positive(runs, expected, failures)
        _compare_handlers(runs, expected, failures)
    return ComparisonSummary(
        example=example,
        status="fail" if failures else "pass",
        runtimes=selected,
        checks=checks,
        failures=tuple(failures),
    )
