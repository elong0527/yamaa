"""Turn retained source snapshots into ordered, typed Polars tables."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.io.csv import CsvProfileFailure, CsvSource, parse_csv
from yamaa.io.parquet import ParquetProfileFailure, parse_parquet
from yamaa.io.polars import frame_from_values
from yamaa.io.profiles import profile_of
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
    requirement: str | None = Field(default=None, pattern=r"^R[0-9]{3}-[0-9]+$")
    context: dict[str, JsonValue]


class SourceError(ValueError):
    """Raised when source declarations or bytes cannot form typed tables."""

    def __init__(self, diagnostics: list[SourceDiagnostic]) -> None:
        if not diagnostics:
            raise ValueError("SourceError requires at least one diagnostic")
        self.diagnostics = tuple(diagnostics)
        conditions = ", ".join(item.condition for item in diagnostics)
        super().__init__(f"source ingestion failed: {conditions}")


class ProducerSchemaUnresolved(ValueError):
    def __init__(self, datasets: tuple[str, ...]) -> None:
        self.datasets = datasets
        super().__init__(
            "producer-linked sources require workflow resolution: "
            + ", ".join(self.datasets)
        )


class LoadedDataset(_FrozenModel):
    """One dataset declaration bound to snapshot bytes and a typed table."""

    dataset: str = Field(min_length=1)
    written_path: str = Field(min_length=1)
    snapshot: ResourceSnapshot
    table: TypedTable


class ProducerField(_FrozenModel):
    """One stored field supplied by a producing specification."""

    name: str = Field(min_length=1)
    type: ColumnType
    label: str = Field(min_length=1)


class ProducerContract(_FrozenModel):
    """The ordered R014 output contract of one producer."""

    fields: tuple[ProducerField, ...] = Field(min_length=1)


def _path_diagnostic(
    dataset: str, written_path: str, failure: ResourceFailure
) -> SourceDiagnostic:
    return SourceDiagnostic(
        phase=failure.phase,
        condition=failure.condition,
        spec_paths=(f"input.{dataset}.path",),
        requirement=failure.requirement,
        context={"dataset": dataset, "path": written_path},
    )


def _csv_diagnostic(
    dataset: str, written_path: str, failure: CsvProfileFailure
) -> SourceDiagnostic:
    return SourceDiagnostic(
        phase="ingest",
        condition=failure.condition,
        spec_paths=(f"input.{dataset}.path",),
        requirement=failure.requirement,
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
        spec_paths=(f"input.{dataset}.path",),
        requirement="R023-23",
        context={"dataset": dataset, "path": written_path},
    )


def _parquet_diagnostic(
    dataset: str, written_path: str, failure: ParquetProfileFailure
) -> SourceDiagnostic:
    return SourceDiagnostic(
        phase="ingest",
        condition=failure.condition,
        spec_paths=(f"input.{dataset}.path",),
        requirement=failure.requirement,
        context={"dataset": dataset, "path": written_path, **failure.context},
    )


def _validate_producer_names(
    dataset: str,
    names: tuple[str, ...],
    contract: ProducerContract,
) -> None:
    expected = tuple(field.name for field in contract.fields)
    if names == expected:
        return
    missing = [name for name in expected if name not in names]
    extra = [name for name in names if name not in expected]
    raise SourceError(
        [
            SourceDiagnostic(
                phase="validation",
                condition="producer_contract_mismatch",
                spec_paths=(
                    f"input.{dataset}.schema",
                    f"input.{dataset}.path",
                ),
                requirement="R014-22",
                context={
                    "dataset": dataset,
                    "expected": list(expected),
                    "actual": list(names),
                    "missing": missing,
                    "extra": extra,
                    "reordered": not missing and not extra,
                },
            )
        ]
    )


def _field_types(
    dataset: str,
    source: DatasetSource,
    parsed: CsvSource,
    contract: ProducerContract | None = None,
) -> tuple[TypedColumn, ...]:
    if contract is not None:
        _validate_producer_names(dataset, parsed.names, contract)
        return tuple(
            TypedColumn(name=field.name, type=field.type) for field in contract.fields
        )
    names = parsed.names
    declared = source.types or {}
    for field in declared:
        if field not in names:
            raise SourceError(
                [
                    SourceDiagnostic(
                        phase="validation",
                        condition="unknown_field",
                        spec_paths=(f"input.{dataset}.types.{field}",),
                        requirement="R014-19",
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
                    spec_paths=(f"input.{dataset}.types.{name}",),
                    requirement="R014-23",
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


def _build_table(
    dataset: str,
    source: DatasetSource,
    parsed: CsvSource,
    contract: ProducerContract | None = None,
) -> TypedTable:
    columns = _field_types(dataset, source, parsed, contract)
    rows = [
        [
            _parse_field(dataset, column.name, column.type, record[index])
            for index, column in enumerate(columns)
        ]
        for record in parsed.records
    ]
    return frame_from_values(columns, rows)


def _validate_parquet_contract(
    dataset: str,
    table: TypedTable,
    contract: ProducerContract | None,
) -> TypedTable:
    if contract is None:
        return table
    names = tuple(column.name for column in table.columns)
    _validate_producer_names(dataset, names, contract)
    diagnostics = [
        SourceDiagnostic(
            phase="validation",
            condition="producer_contract_mismatch",
            spec_paths=(
                f"input.{dataset}.schema",
                f"input.{dataset}.path",
            ),
            requirement="R014-22",
            context={
                "dataset": dataset,
                "field": actual.name,
                "expected_type": expected.type,
                "actual_type": actual.type,
            },
        )
        for actual, expected in zip(table.columns, contract.fields, strict=True)
        if actual.type != expected.type
    ]
    if diagnostics:
        raise SourceError(diagnostics)
    return table


def load_source_tables(
    datasets: Mapping[str, DatasetSource],
    resources: ProjectResources,
    *,
    producer_contracts: Mapping[str, ProducerContract] | None = None,
    producer_snapshots: Mapping[str, ResourceSnapshot] | None = None,
) -> dict[str, LoadedDataset]:
    """Capture, verify, and ingest normalized dataset declarations."""
    contracts = producer_contracts or {}
    snapshots = producer_snapshots or {}
    diagnostics: list[SourceDiagnostic] = []
    for dataset, source in datasets.items():
        profile = profile_of(source.path)
        if source.schema_path is not None and source.types is not None:
            diagnostics.extend(
                SourceDiagnostic(
                    phase="validation",
                    condition="redundant_field_type",
                    spec_paths=(f"input.{dataset}.types.{field}",),
                    requirement="R014-10",
                    context={
                        "dataset": dataset,
                        "field": field,
                        "type": value,
                    },
                )
                for field, value in source.types.items()
            )
        elif profile == "parquet" and source.types is not None:
            diagnostics.extend(
                SourceDiagnostic(
                    phase="validation",
                    condition="redundant_field_type",
                    spec_paths=(f"input.{dataset}.types.{field}",),
                    requirement="R014-20",
                    context={
                        "dataset": dataset,
                        "field": field,
                        "type": value,
                    },
                )
                for field, value in source.types.items()
            )
        try:
            if dataset in snapshots:
                resources.validate_location(source.path)
            else:
                resources.validate(source.path)
            if profile is None:
                diagnostics.append(_profile_diagnostic(dataset, source.path))
        except ResourceFailure as failure:
            diagnostics.append(_path_diagnostic(dataset, source.path, failure))
    if diagnostics:
        raise SourceError(diagnostics)

    unresolved = tuple(
        dataset
        for dataset, source in datasets.items()
        if source.schema_path is not None and dataset not in contracts
    )
    if unresolved:
        raise ProducerSchemaUnresolved(unresolved)

    seen: set[int] = set()
    loaded: dict[str, LoadedDataset] = {}
    for dataset, source in datasets.items():
        try:
            injected = snapshots.get(dataset)
            snapshot = injected or resources.capture(source.path)
            if injected is None and id(snapshot) not in seen:
                resources.verify(snapshot)
            seen.add(id(snapshot))
            profile = profile_of(source.path)
            assert profile is not None
            if profile == "csv":
                parsed = parse_csv(snapshot.content)
                table = _build_table(dataset, source, parsed, contracts.get(dataset))
            else:
                table = _validate_parquet_contract(
                    dataset,
                    parse_parquet(snapshot.content),
                    contracts.get(dataset),
                )
        except ResourceFailure as failure:
            diagnostics.append(_path_diagnostic(dataset, source.path, failure))
            continue
        except CsvProfileFailure as failure:
            diagnostics.append(_csv_diagnostic(dataset, source.path, failure))
            continue
        except ParquetProfileFailure as failure:
            diagnostics.append(_parquet_diagnostic(dataset, source.path, failure))
            continue
        except SourceError as error:
            diagnostics.extend(error.diagnostics)
            continue
        loaded[dataset] = LoadedDataset(
            dataset=dataset,
            written_path=source.path,
            snapshot=snapshot,
            table=table,
        )
    if diagnostics:
        raise SourceError(diagnostics)
    return loaded


def load_source_table(
    dataset: str,
    source: DatasetSource,
    resources: ProjectResources,
    *,
    producer_contract: ProducerContract | None = None,
    producer_snapshot: ResourceSnapshot | None = None,
) -> LoadedDataset:
    """Load one normalized dataset declaration."""
    contracts = {dataset: producer_contract} if producer_contract is not None else None
    snapshots = {dataset: producer_snapshot} if producer_snapshot is not None else None
    return load_source_tables(
        {dataset: source},
        resources,
        producer_contracts=contracts,
        producer_snapshots=snapshots,
    )[dataset]
