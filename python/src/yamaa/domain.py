"""User-facing execution of one yamaa domain specification."""

from __future__ import annotations

import json
from collections.abc import Callable, Iterable, Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path

import polars as pl

from yamaa.application import PreparedWorkflow, discover_schema_root, prepare_workflow
from yamaa.expressions import ExpressionDispatcher
from yamaa.io import (
    ArtifactTarget,
    LoadedDataset,
    build_artifact,
    publish_artifact,
)
from yamaa.planning import (
    ExecutionDiagnostic,
    UnsupportedFeature,
    execute_workflow,
)
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionResult,
    ExecutionSuccess,
    ExecutionUnsupported,
)
from yamaa.specification import (
    Specification,
    SpecificationError,
    ValidationDiagnostic,
)

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


_discover_schema_root = discover_schema_root


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
    def warning_log(self) -> pl.DataFrame | None:
        """Return the R009 warning sidecar frame, or ``None`` when absent."""
        if not isinstance(self._result, ExecutionSuccess):
            return None
        if self._result.warning_log is None:
            return None
        return self._result.warning_log.frame.clone()

    @property
    def verification_log(self) -> pl.DataFrame | None:
        """Return evaluated checks, including a retained log after failure."""
        if not isinstance(self._result, (ExecutionSuccess, ExecutionFailure)):
            return None
        if self._result.verification_log is None:
            return None
        return self._result.verification_log.frame.clone()

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
    dispatcher: ExpressionDispatcher | None = None,
    study_document: str | Path | None = None,
) -> DomainRun:
    """Load, validate, and execute one domain specification exactly once.

    ``dispatcher`` is a generic expression-dispatch hook the engine threads
    through unchanged: extensions that evaluate operations the core language
    does not implement (project functions, for example) build the dispatcher
    and hand it in. The engine itself never imports or orchestrates such an
    extension. Without it, a specification calling project functions reports
    ``function`` as an unimplemented operation (REQ-0662).

    ``study_document`` explicitly supplies the codelists bound by submission
    metadata. No adjacent study document is discovered implicitly. Bound
    non-extensible lists are checked at each column's verification boundary.
    """
    return _run_domain(
        entry_path,
        schema_root=schema_root,
        project_root=project_root,
        data_roots=data_roots,
        read_project_configuration=read_project_configuration,
        dispatcher=dispatcher,
        study_document=study_document,
    )


def _run_domain(
    entry_path: str | Path,
    *,
    schema_root: str | Path | None = None,
    project_root: str | Path | None = None,
    data_roots: Iterable[str | Path] | None = None,
    read_project_configuration: bool = True,
    dispatcher: ExpressionDispatcher | None = None,
    study_document: str | Path | None = None,
    dispatcher_factory: Callable[[PreparedWorkflow], ExpressionDispatcher | None]
    | None = None,
    raise_specification_errors: bool = False,
) -> DomainRun:
    """Run the shared lifecycle with an optional externally owned activation step."""
    try:
        prepared = prepare_workflow(
            entry_path,
            schema_root=schema_root,
            project_root=project_root,
            data_roots=data_roots,
            read_project_configuration=read_project_configuration,
        )
    except SpecificationError as error:
        if raise_specification_errors:
            raise
        issues = _issues_frame(_diagnostic_rows(error.diagnostics))
        return DomainRun(Path(entry_path).resolve(), None, {}, None, issues)
    entry, selected_schema = prepared.entry, prepared.schema_root
    resources, workflow = prepared.resources, prepared.workflow
    if dispatcher_factory is not None:
        activated_dispatcher = dispatcher_factory(prepared)
        if activated_dispatcher is not None:
            if dispatcher is not None:
                raise TypeError(
                    "a project-function run cannot also supply a dispatcher"
                )
            dispatcher = activated_dispatcher

    entry_node = next(
        node for node in workflow.nodes if node.entry_path == workflow.entry_path
    )
    hooks_for_specification = None
    if study_document is not None:
        from yamaa.specification.terminology import (
            load_study_document,
            validate_spec_codelist_bindings,
        )
        from yamaa.submission.verification import submission_hooks

        try:
            study = load_study_document(study_document, selected_schema)
            for node in workflow.nodes:
                diagnostics = validate_spec_codelist_bindings(
                    node.resolved.specification, study.document
                )
                if diagnostics:
                    raise SpecificationError(diagnostics)
        except SpecificationError as error:
            return DomainRun(
                entry,
                entry_node.resolved.specification,
                {},
                None,
                _issues_frame(_diagnostic_rows(error.diagnostics)),
            )
        hooks_for_specification = lambda spec: submission_hooks(study.document, spec)
    execution = execute_workflow(
        workflow,
        resources,
        dispatcher=dispatcher,
        hooks_for_specification=hooks_for_specification,
    )
    sources = dict(execution.sources.get(workflow.entry_path, {}))
    result = execution.result
    return DomainRun(
        entry,
        entry_node.resolved.specification,
        sources,
        result,
        _result_issues(result),
    )


__all__ = ["DomainRun", "DomainRunError", "yamaa_domain"]
