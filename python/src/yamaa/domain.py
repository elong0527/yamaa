"""User-facing execution of one YAMAA domain specification."""

from __future__ import annotations

import json
from collections.abc import Iterable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

import polars as pl

from yamaa.io import (
    ArtifactTarget,
    LoadedDataset,
    ProjectResources,
    approve_roots,
    build_artifact,
    load_source_tables,
    publish_artifact,
)
from yamaa.planning import ExecutionDiagnostic, UnsupportedFeature
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionResult,
    ExecutionSuccess,
    ExecutionUnsupported,
    execute_with_source_provider,
)
from yamaa.specification import (
    Specification,
    SpecificationError,
    ValidationDiagnostic,
    load_specification,
)
from yamaa.specification.models import DatasetSource

_ISSUE_COLUMNS = (
    "severity",
    "phase",
    "condition",
    "spec_paths",
    "context",
)


def _issues_frame(rows: Sequence[tuple[object, ...]]) -> pl.DataFrame:
    values = list(rows)
    return pl.DataFrame(
        {
            "severity": pl.Series(
                "severity", [row[0] for row in values], dtype=pl.String
            ),
            "phase": pl.Series("phase", [row[1] for row in values], dtype=pl.String),
            "condition": pl.Series(
                "condition", [row[2] for row in values], dtype=pl.String
            ),
            "spec_paths": pl.Series(
                "spec_paths", [row[3] for row in values], dtype=pl.List(pl.String)
            ),
            "context": pl.Series(
                "context", [row[4] for row in values], dtype=pl.String
            ),
        }
    ).select(_ISSUE_COLUMNS)


def _context_text(context: Mapping[str, object]) -> str:
    return json.dumps(context, ensure_ascii=True, sort_keys=True, separators=(",", ":"))


def _diagnostic_rows(
    diagnostics: Sequence[ValidationDiagnostic | ExecutionDiagnostic],
) -> list[tuple[object, ...]]:
    return [
        (
            "error",
            diagnostic.phase,
            diagnostic.condition,
            list(diagnostic.spec_paths),
            _context_text(diagnostic.context),
        )
        for diagnostic in diagnostics
    ]


def _unsupported_rows(
    features: Sequence[UnsupportedFeature],
) -> list[tuple[object, ...]]:
    return [
        (
            "unsupported",
            "planning",
            "unsupported_operation",
            [feature.spec_path],
            _context_text({"operation": feature.operation}),
        )
        for feature in features
    ]


def _result_issues(result: ExecutionResult | None) -> pl.DataFrame:
    if isinstance(result, ExecutionFailure):
        return _issues_frame(_diagnostic_rows(result.diagnostics))
    if isinstance(result, ExecutionUnsupported):
        return _issues_frame(_unsupported_rows(result.features))
    return _issues_frame(())


def _discover_schema_root(entry_path: Path) -> Path:
    candidates = [entry_path.parent, *entry_path.parent.parents, Path.cwd()]
    package_path = Path(__file__).resolve()
    candidates.extend(package_path.parents)
    seen: set[Path] = set()
    for candidate in candidates:
        for root in (candidate, candidate / "yaml"):
            resolved = root.resolve()
            if resolved in seen:
                continue
            seen.add(resolved)
            if (resolved / "schema.yaml").is_file():
                return resolved
    raise ValueError("cannot find the YAMAA schema bundle; pass schema_root explicitly")


class DomainRunError(RuntimeError):
    """Raised when output is requested from an unsuccessful domain run."""

    def __init__(self, issues: pl.DataFrame) -> None:
        self.issues = issues.clone()
        super().__init__("domain execution produced no output; inspect pilot.issues")


@dataclass(frozen=True, slots=True)
class DomainRun:
    """One cached domain-specification run exposed as Polars-facing properties."""

    _entry_path: Path
    _specification: Specification | None
    _sources: Mapping[str, LoadedDataset]
    _result: ExecutionResult | None
    _issues: pl.DataFrame

    @property
    def spec(self) -> Specification | None:
        """Return the normalized domain specification when loading succeeded."""
        return self._specification

    @property
    def inputs(self) -> dict[str, pl.DataFrame]:
        """Return source datasets as declaration-ordered Polars frames."""
        return {
            dataset: source.table.frame.clone()
            for dataset, source in self._sources.items()
        }

    @property
    def input(self) -> dict[str, pl.DataFrame]:
        """Return the input datasets; ``inputs`` is the canonical spelling."""
        return self.inputs

    @property
    def output(self) -> pl.DataFrame | None:
        """Return the ordered output frame, or ``None`` after an unsuccessful run."""
        if not isinstance(self._result, ExecutionSuccess):
            return None
        return self._result.artifact.frame.clone()

    @property
    def issues(self) -> pl.DataFrame:
        """Return all structured run issues in a stable Polars schema."""
        return self._issues.clone()

    def save(self, path: str | Path | None = None) -> Path:
        """Atomically save successful output to the declared or supplied path."""
        if not isinstance(self._result, ExecutionSuccess):
            raise DomainRunError(self._issues)
        assert self._specification is not None

        if path is None:
            target_path = (
                self._entry_path.parent / self._specification.output.path
            ).resolve()
            artifact = self._result.artifact
        else:
            target_path = Path(path).resolve()
            requested_output = self._specification.output.model_copy(
                update={"path": str(target_path)}
            )
            artifact = build_artifact(
                self._result.table,
                requested_output,
                self._specification.keys,
            )
        return publish_artifact(ArtifactTarget(target_path), artifact)


def yamaa_domain(
    entry_path: str | Path,
    *,
    schema_root: str | Path | None = None,
    project_root: str | Path | None = None,
    data_roots: Iterable[str | Path] | None = None,
    read_project_configuration: bool = True,
) -> DomainRun:
    """Load, validate, and execute one domain specification exactly once."""
    entry = Path(entry_path)
    if not entry.is_file():
        raise FileNotFoundError(f"domain specification is not a file: {entry}")
    entry = entry.resolve()
    selected_schema = (
        Path(schema_root).resolve()
        if schema_root is not None
        else _discover_schema_root(entry)
    )
    approved = approve_roots(
        entry,
        project_root=project_root,
        data_roots=data_roots,
        read_project_configuration=read_project_configuration,
    )

    try:
        loaded = load_specification(entry, selected_schema)
    except SpecificationError as error:
        issues = _issues_frame(_diagnostic_rows(error.diagnostics))
        return DomainRun(entry, None, {}, None, issues)

    resources = ProjectResources(
        approved.project_root,
        base_directory=entry.parent,
        data_roots=approved.data_roots,
    )
    sources: dict[str, LoadedDataset] = {}

    def provide(
        datasets: Mapping[str, DatasetSource],
    ) -> Mapping[str, LoadedDataset]:
        loaded_sources = load_source_tables(datasets, resources)
        sources.update(loaded_sources)
        return loaded_sources

    result = execute_with_source_provider(loaded.specification, provide)
    return DomainRun(
        entry,
        loaded.specification,
        sources,
        result,
        _result_issues(result),
    )


__all__ = ["DomainRun", "DomainRunError", "yamaa_domain"]
