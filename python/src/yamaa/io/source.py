"""Turn retained source snapshots into ordered, typed Polars tables."""

from __future__ import annotations

from collections.abc import Mapping
from pathlib import PurePosixPath
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.io.csv import CsvProfileFailure, CsvSource, parse_csv
from yamaa.io.polars import frame_from_values
from yamaa.io.project import (
    ProjectResources,
    ResourceFailure,
    ResourceSnapshot,
)
from yamaa.models import (
    MISSING,
    ConditionResult,
    TypedColumn,
    TypedTable,
    ValueResult,
    convert_value,
)
from yamaa.specification.models import ColumnType, DatasetSource


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class SourceDiagnostic(_FrozenModel):
    """Stable source validation or ingestion failure."""

    phase: Literal["validation", "ingest"]
    condition: str = Field(min_length=1)
    spec_paths: tuple[str, ...] = Field(min_length=1)
    context: dict[str, JsonValue]


class SourceError(ValueError):
    """Raised when source declarations or bytes cannot form typed tables."""

    def __init__(self, diagnostics: list[SourceDiagnostic]) -> None:
        if not diagnostics:
            raise ValueError("SourceError requires at least one diagnostic")
        self.diagnostics = tuple(diagnostics)
        conditions = ", ".join(item.condition for item in diagnostics)
        super().__init__(f"source ingestion failed: {conditions}")


class LoadedDataset(_FrozenModel):
    """One dataset declaration bound to snapshot bytes and a typed table."""

    dataset: str = Field(min_length=1)
    written_path: str = Field(min_length=1)
    snapshot: ResourceSnapshot
    table: TypedTable


def _path_diagnostic(
    dataset: str, written_path: str, failure: ResourceFailure
) -> SourceDiagnostic:
    return SourceDiagnostic(
        phase=failure.phase,
        condition=failure.condition,
        spec_paths=(f"datasets.{dataset}.path",),
        context={"dataset": dataset, "path": written_path},
    )


def _csv_diagnostic(
    dataset: str, written_path: str, failure: CsvProfileFailure
) -> SourceDiagnostic:
    return SourceDiagnostic(
        phase="ingest",
        condition=failure.condition,
        spec_paths=(f"datasets.{dataset}.path",),
        context={
            "dataset": dataset,
            "path": written_path,
            "record": failure.record,
            "field": failure.field,
        },
    )


def _profile_diagnostic(dataset: str, written_path: str) -> SourceDiagnostic:
    return SourceDiagnostic(
        phase="validation",
        condition="source_profile_unknown",
        spec_paths=(f"datasets.{dataset}.path",),
        context={"dataset": dataset, "path": written_path},
    )


def _field_types(
    dataset: str, source: DatasetSource, parsed: CsvSource
) -> tuple[TypedColumn, ...]:
    names = parsed.names
    declared = source.types or {}
    for field in declared:
        if field not in names:
            raise SourceError(
                [
                    SourceDiagnostic(
                        phase="validation",
                        condition="unknown_field",
                        spec_paths=(f"datasets.{dataset}.types.{field}",),
                        context={"dataset": dataset, "field": field},
                    )
                ]
            )
    return tuple(
        TypedColumn(name=name, type=declared.get(name, "str")) for name in names
    )


def _parse_field(
    dataset: str,
    name: str,
    target: ColumnType,
    text: str | None,
) -> object:
    if text is None:
        return None
    converted = convert_value(text, target)
    if isinstance(converted, ConditionResult):
        raise SourceError(
            [
                SourceDiagnostic(
                    phase="ingest",
                    condition="field_parse_failed",
                    spec_paths=(f"datasets.{dataset}.types.{name}",),
                    context={
                        "dataset": dataset,
                        "field": name,
                        "type": target,
                        "value": text,
                    },
                )
            ]
        )
    assert isinstance(converted, ValueResult)
    return None if converted.value is MISSING else converted.value


def _build_table(dataset: str, source: DatasetSource, parsed: CsvSource) -> TypedTable:
    columns = _field_types(dataset, source, parsed)
    rows = [
        [
            _parse_field(dataset, column.name, column.type, record[index])
            for index, column in enumerate(columns)
        ]
        for record in parsed.records
    ]
    return frame_from_values(columns, rows)


def load_source_tables(
    datasets: Mapping[str, DatasetSource],
    resources: ProjectResources,
) -> dict[str, LoadedDataset]:
    """Capture, verify, and ingest normalized CSV dataset declarations."""
    if any(source.schema_path is not None for source in datasets.values()):
        raise NotImplementedError("producer-linked sources require workflow resolution")

    diagnostics: list[SourceDiagnostic] = []
    for dataset, source in datasets.items():
        try:
            resources.validate(source.path)
            if PurePosixPath(source.path).suffix.lower() != ".csv":
                diagnostics.append(_profile_diagnostic(dataset, source.path))
        except ResourceFailure as failure:
            diagnostics.append(_path_diagnostic(dataset, source.path, failure))
    if diagnostics:
        raise SourceError(diagnostics)

    captured: dict[str, ResourceSnapshot] = {}
    for dataset, source in datasets.items():
        try:
            captured[dataset] = resources.capture(source.path)
        except ResourceFailure as failure:
            diagnostics.append(_path_diagnostic(dataset, source.path, failure))
    if diagnostics:
        raise SourceError(diagnostics)

    verified: set[int] = set()
    for dataset, snapshot in captured.items():
        if id(snapshot) in verified:
            continue
        try:
            resources.verify(snapshot)
        except ResourceFailure as failure:
            failed_dataset = next(
                (
                    name
                    for name, source in datasets.items()
                    if source.path == failure.written_path
                ),
                dataset,
            )
            raise SourceError(
                [
                    _path_diagnostic(
                        failed_dataset,
                        datasets[failed_dataset].path,
                        failure,
                    )
                ]
            ) from failure
        verified.add(id(snapshot))

    loaded: dict[str, LoadedDataset] = {}
    for dataset, source in datasets.items():
        snapshot = captured[dataset]
        try:
            parsed = parse_csv(snapshot.content)
        except CsvProfileFailure as failure:
            raise SourceError(
                [_csv_diagnostic(dataset, source.path, failure)]
            ) from failure
        loaded[dataset] = LoadedDataset(
            dataset=dataset,
            written_path=source.path,
            snapshot=snapshot,
            table=_build_table(dataset, source, parsed),
        )
    return loaded


def load_source_table(
    dataset: str,
    source: DatasetSource,
    resources: ProjectResources,
) -> LoadedDataset:
    """Load one normalized CSV dataset declaration."""
    return load_source_tables({dataset: source}, resources)[dataset]
