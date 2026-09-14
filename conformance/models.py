"""Version 1.0 invocation, report, and comparison-result models."""

from __future__ import annotations

import datetime as dt
import re
from pathlib import Path, PurePosixPath
from typing import Annotated, Literal

from pydantic import (
    BaseModel,
    ConfigDict,
    Field,
    JsonValue,
    model_validator,
)

PROTOCOL_VERSION = "1.0"
SHA256_PATTERN = r"^[0-9a-f]{64}$"
NAME_PATTERN = r"^[a-z][a-z0-9_-]*$"
EXAMPLE_PATTERN = r"^[a-z0-9]+(?:-[a-z0-9]+)*$"
REQUIREMENT_PATTERN = r"^R[0-9]{3}-[0-9]+$"
INTEGER_PATTERN = re.compile(r"^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$")
FLOAT_BITS_PATTERN = re.compile(r"^[0-9a-f]{16}$")
DATE_PATTERN = re.compile(r"^[0-9]{4}-[0-9]{2}-[0-9]{2}$")
DATETIME_PATTERN = re.compile(
    r"^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}$"
)

ColumnType = Literal["str", "int", "float", "date", "datetime"]
ConditionPhase = Literal[
    "validation",
    "ingest",
    "row_construction",
    "derivation",
    "output",
    "verification",
    "bind",
    "join",
    "mapping",
    "cut",
    "extract",
    "template",
    "impute",
    "convert",
    "final",
]
TemporalPrecision = Literal["year", "month", "day", "second"]


class ContractModel(BaseModel):
    """Strict base for every object crossing the runner boundary."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


def _portable_relative_path(value: str) -> str:
    if not value or "\\" in value:
        raise ValueError("must be a non-empty POSIX relative path")
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or str(path) != value:
        raise ValueError("must be a normalized POSIX relative path without '..'")
    return value


class Invocation(ContractModel):
    """One runtime's request to execute one example specification."""

    protocol_version: Literal["1.0"] = PROTOCOL_VERSION
    run_id: str = Field(min_length=1, max_length=128)
    example: str = Field(pattern=EXAMPLE_PATTERN)
    runtime: str = Field(pattern=NAME_PATTERN)
    project_root: str = Field(min_length=1)
    schema_root: str = Field(min_length=1)
    entrypoint: str = Field(min_length=1)
    data_roots: tuple[str, ...] = ()
    output_directory: str = Field(min_length=1)

    @model_validator(mode="after")
    def validate_paths(self) -> Invocation:
        project = Path(self.project_root)
        output = Path(self.output_directory)
        if not project.is_absolute():
            raise ValueError("project_root must be absolute")
        if not output.is_absolute():
            raise ValueError("output_directory must be absolute")
        _portable_relative_path(self.schema_root)
        _portable_relative_path(self.entrypoint)
        if len(set(self.data_roots)) != len(self.data_roots):
            raise ValueError("data_roots must not contain duplicates")
        if any(not Path(root).is_absolute() for root in self.data_roots):
            raise ValueError("every data root must be absolute")

        expected = project / PurePosixPath(self.entrypoint).parent / "expected"
        try:
            output.relative_to(expected)
        except ValueError:
            pass
        else:
            raise ValueError("output_directory must not be inside expected/")
        return self


class FileDigest(ContractModel):
    """A portable path and the SHA-256 of the bytes read from it."""

    path: str = Field(min_length=1)
    sha256: str = Field(pattern=SHA256_PATTERN)

    @model_validator(mode="after")
    def validate_path(self) -> FileDigest:
        _portable_relative_path(self.path)
        return self


class ResolvedSpecification(ContractModel):
    """A decoded R017 result serialized as JSON in the runtime directory."""

    path: str = Field(min_length=1)
    sha256: str = Field(pattern=SHA256_PATTERN)

    @model_validator(mode="after")
    def validate_path(self) -> ResolvedSpecification:
        _portable_relative_path(self.path)
        return self


class SpecificationObservation(ContractModel):
    """The authored documents and optional resolved specification used."""

    entrypoint: str = Field(min_length=1)
    documents: tuple[FileDigest, ...] = Field(min_length=1)
    resolved: ResolvedSpecification | None = None

    @model_validator(mode="after")
    def validate_documents(self) -> SpecificationObservation:
        _portable_relative_path(self.entrypoint)
        paths = [item.path for item in self.documents]
        if len(paths) != len(set(paths)):
            raise ValueError("specification document paths must be unique")
        if self.entrypoint not in paths:
            raise ValueError("specification documents must include the entrypoint")
        return self


class SourceObservation(ContractModel):
    """One source byte snapshot consumed under a dataset identifier."""

    dataset: str = Field(min_length=1)
    declared_path: str = Field(min_length=1)
    sha256: str = Field(pattern=SHA256_PATTERN)


class RuntimeInformation(ContractModel):
    """Runtime and engine identity recorded without host-specific details."""

    name: str = Field(pattern=NAME_PATTERN)
    language_version: str = Field(min_length=1)
    implementation_version: str = Field(min_length=1)


class CellObservation(ContractModel):
    """One typed value, with missingness separate from its portable value."""

    missing: bool
    value: str | None = None
    precision: TemporalPrecision | None = None

    @model_validator(mode="after")
    def validate_missingness(self) -> CellObservation:
        if self.missing and (self.value is not None or self.precision is not None):
            raise ValueError("a missing cell has neither value nor precision")
        if not self.missing and self.value is None:
            raise ValueError("a non-missing cell requires value")
        return self


class ColumnObservation(ContractModel):
    """One artifact column in artifact order."""

    name: str = Field(min_length=1)
    type: ColumnType


class TableObservation(ContractModel):
    """An ordered, typed table independent of its serialized container."""

    columns: tuple[ColumnObservation, ...] = Field(min_length=1)
    rows: tuple[tuple[CellObservation, ...], ...]

    @model_validator(mode="after")
    def validate_table(self) -> TableObservation:
        names = [column.name for column in self.columns]
        if len(names) != len(set(names)):
            raise ValueError("table column names must be unique")
        for row_index, row in enumerate(self.rows):
            if len(row) != len(self.columns):
                raise ValueError(
                    f"row {row_index} has {len(row)} cells for "
                    f"{len(self.columns)} columns"
                )
            for column, cell in zip(self.columns, row, strict=True):
                _validate_cell(column, cell)
        return self


def _validate_cell(column: ColumnObservation, cell: CellObservation) -> None:
    if cell.missing:
        return
    assert cell.value is not None
    value = cell.value
    if column.type == "int" and INTEGER_PATTERN.fullmatch(value) is None:
        raise ValueError(f"{column.name}: int value must use canonical decimal text")
    if column.type == "float" and FLOAT_BITS_PATTERN.fullmatch(value) is None:
        raise ValueError(
            f"{column.name}: float value must be 16 lowercase IEEE-754 hex digits"
        )
    if column.type == "date":
        if DATE_PATTERN.fullmatch(value) is None:
            raise ValueError(f"{column.name}: date value must use R016 text")
        try:
            dt.date.fromisoformat(value)
        except ValueError as error:
            raise ValueError(f"{column.name}: invalid calendar date") from error
        if cell.precision not in {"year", "month", "day"}:
            raise ValueError(f"{column.name}: date value requires collected precision")
    elif column.type == "datetime":
        if DATETIME_PATTERN.fullmatch(value) is None:
            raise ValueError(f"{column.name}: datetime value must use R016 text")
        try:
            dt.datetime.fromisoformat(value)
        except ValueError as error:
            raise ValueError(f"{column.name}: invalid civil datetime") from error
        if cell.precision != "second":
            raise ValueError(f"{column.name}: datetime precision must be second")
    elif cell.precision is not None:
        raise ValueError(f"{column.name}: only temporal values carry precision")


class ArtifactObservation(ContractModel):
    """One isolated artifact plus its ordered typed-table observation."""

    role: Literal["primary", "violation_log"]
    declared_path: str = Field(min_length=1)
    produced_path: str = Field(min_length=1)
    profile: Literal["csv", "parquet"]
    sha256: str = Field(pattern=SHA256_PATTERN)
    table: TableObservation

    @model_validator(mode="after")
    def validate_path(self) -> ArtifactObservation:
        _portable_relative_path(self.produced_path)
        return self


class Diagnostic(ContractModel):
    """Portable diagnostic identity; messages and exception names stay out."""

    severity: Literal["error", "warning"]
    phase: ConditionPhase
    condition: str = Field(min_length=1)
    spec_paths: tuple[str, ...] = Field(min_length=1)
    requirement: str | None = Field(default=None, pattern=REQUIREMENT_PATTERN)
    context: dict[str, JsonValue]


class HandlerCount(ContractModel):
    """The number of records that used one declared handler path."""

    spec_path: str = Field(min_length=1)
    handler: str = Field(min_length=1)
    count: int = Field(ge=0)


class UnsupportedFeature(ContractModel):
    """One valid construct the adapter cannot execute."""

    operation: str = Field(min_length=1)
    spec_path: str = Field(min_length=1)


class SuccessOutcome(ContractModel):
    status: Literal["success"]
    artifacts: tuple[ArtifactObservation, ...] = Field(min_length=1)
    diagnostics: tuple[Diagnostic, ...] = ()

    @model_validator(mode="after")
    def validate_success(self) -> SuccessOutcome:
        roles = [artifact.role for artifact in self.artifacts]
        if roles.count("primary") != 1 or len(roles) != len(set(roles)):
            raise ValueError("success requires one primary and unique artifact roles")
        if any(item.severity == "error" for item in self.diagnostics):
            raise ValueError("a successful outcome cannot contain an error")
        return self


class FailureOutcome(ContractModel):
    status: Literal["failure"]
    diagnostics: tuple[Diagnostic, ...] = Field(min_length=1)

    @model_validator(mode="after")
    def validate_failure(self) -> FailureOutcome:
        if not any(item.severity == "error" for item in self.diagnostics):
            raise ValueError("a failed outcome requires an error diagnostic")
        return self


class UnsupportedOutcome(ContractModel):
    status: Literal["unsupported"]
    features: tuple[UnsupportedFeature, ...] = Field(min_length=1)


class BlockedOutcome(ContractModel):
    status: Literal["blocked"]
    blockers: tuple[str, ...] = Field(min_length=1)

    @model_validator(mode="after")
    def validate_blockers(self) -> BlockedOutcome:
        if any(re.fullmatch(r"#[1-9][0-9]*", item) is None for item in self.blockers):
            raise ValueError("every blocker must be an issue reference such as #215")
        if len(self.blockers) != len(set(self.blockers)):
            raise ValueError("blockers must be unique")
        return self


class InfrastructureFailureOutcome(ContractModel):
    status: Literal["infrastructure_failure"]
    condition: str = Field(min_length=1)
    context: dict[str, JsonValue]


Outcome = Annotated[
    SuccessOutcome
    | FailureOutcome
    | UnsupportedOutcome
    | BlockedOutcome
    | InfrastructureFailureOutcome,
    Field(discriminator="status"),
]


class Report(ContractModel):
    """One adapter's complete observation of one invocation."""

    protocol_version: Literal["1.0"] = PROTOCOL_VERSION
    invocation_sha256: str = Field(pattern=SHA256_PATTERN)
    example: str = Field(pattern=EXAMPLE_PATTERN)
    runtime: RuntimeInformation
    specification: SpecificationObservation | None
    sources: tuple[SourceObservation, ...] = ()
    handler_counts: tuple[HandlerCount, ...] = ()
    outcome: Outcome

    @model_validator(mode="after")
    def validate_report(self) -> Report:
        datasets = [item.dataset for item in self.sources]
        if len(datasets) != len(set(datasets)):
            raise ValueError("source dataset observations must be unique")
        handler_keys = [(item.spec_path, item.handler) for item in self.handler_counts]
        if len(handler_keys) != len(set(handler_keys)):
            raise ValueError("handler-count paths and names must be unique")

        semantic = isinstance(
            self.outcome, (SuccessOutcome, FailureOutcome, UnsupportedOutcome)
        )
        if semantic and self.specification is None:
            raise ValueError("semantic outcomes require a specification observation")
        if isinstance(self.outcome, (SuccessOutcome, UnsupportedOutcome)):
            assert self.specification is not None
            if self.specification.resolved is None:
                raise ValueError("success and unsupported outcomes require resolution")
        if isinstance(self.outcome, FailureOutcome):
            later_error = any(
                item.severity == "error" and item.phase != "validation"
                for item in self.outcome.diagnostics
            )
            validation_error = any(
                item.severity == "error" and item.phase == "validation"
                for item in self.outcome.diagnostics
            )
            assert self.specification is not None
            if later_error and self.specification.resolved is None:
                raise ValueError("post-validation failures require resolution")
            if validation_error and self.sources:
                raise ValueError("validation failures cannot report consumed sources")
        return self


class ComparisonSummary(ContractModel):
    """Concise per-example result suitable for a CI artifact."""

    protocol_version: Literal["1.0"] = PROTOCOL_VERSION
    example: str
    status: Literal["pass", "fail"]
    runtimes: tuple[str, ...]
    checks: tuple[str, ...]
    failures: tuple[str, ...]
