"""Turn retained source snapshots into ordered, typed Polars tables."""

from __future__ import annotations

from collections.abc import Collection, Mapping, Sequence
from pathlib import Path
from typing import Literal

import polars as pl
from pydantic import BaseModel, ConfigDict, Field, JsonValue

from yamaa.io.csv import CsvProfileFailure, CsvSource, parse_csv
from yamaa.io.parquet import (
    ParquetProfileFailure,
    parquet_source_fields,
    parquet_text_columns,
    parse_parquet,
)
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
    requirement: str | None = Field(
        default=None, pattern=r"^(?:REQ-[0-9]{4,}|R[0-9]{3}-[1-9][0-9]*[a-z]?)$"
    )
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
    dataset: str,
    written_path: str,
    failure: ResourceFailure,
    *,
    data_roots_checked: int = 0,
) -> SourceDiagnostic:
    context: dict[str, object] = {"dataset": dataset, "path": written_path}
    if failure.condition == "resource_path_missing" and data_roots_checked:
        context["data_roots_checked"] = data_roots_checked
    return SourceDiagnostic(
        phase=failure.phase,
        condition=failure.condition,
        spec_paths=(f"input.{dataset}.path",),
        requirement=failure.requirement,
        context=context,
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
        requirement="REQ-0852",
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
                requirement="REQ-0535",
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
                        requirement="REQ-0532",
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
                    requirement="REQ-0536",
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


def _empty_strings_to_missing(table: TypedTable) -> TypedTable:
    """Map stored empty strings to missing in `str` columns (REQ-1159).

    The Parquet profile keeps a zero-length string distinct from null
    (REQ-1034); the input-side convention decides what the engine sees.
    """
    names = [column.name for column in table.columns if column.type == "str"]
    if not names:
        return table
    frame = table.frame.with_columns(
        pl.when(pl.col(name) == "").then(None).otherwise(pl.col(name)).alias(name)
        for name in names
    )
    return TypedTable(columns=table.columns, frame=frame)


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
            requirement="REQ-0535",
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


def _odm_field_diagnostics(
    dataset: str,
    names: Sequence[str],
    stored_types: Mapping[str, ColumnType | None] | None,
) -> tuple[dict[str, str], list[SourceDiagnostic]]:
    """Bind stored fields to the ODM schema and report what REQ-1268 rejects.

    ``stored_types`` is None for a CSV source, whose every field is text; a
    Parquet source gives each field's type, None where the closed mapping
    has none.
    """
    # The ODM package reads loaded datasets, so it is imported here rather
    # than at module load, which would make the two modules import each other.
    from yamaa.odm.items import bind_fields

    binding = bind_fields(names)
    diagnostics: list[SourceDiagnostic] = []
    path = (f"input.{dataset}.path",)
    if binding.missing:
        diagnostics.append(
            SourceDiagnostic(
                phase="validation",
                condition="odm_schema_field_missing",
                spec_paths=path,
                requirement="REQ-1275",
                context={"dataset": dataset, "fields": list(binding.missing)},
            )
        )
    diagnostics.extend(
        SourceDiagnostic(
            phase="validation",
            condition="odm_schema_field_ambiguous",
            spec_paths=path,
            requirement="REQ-1275",
            context={"dataset": dataset, "field": field, "stored": list(stored)},
        )
        for field, stored in binding.ambiguous.items()
    )
    if stored_types is not None:
        diagnostics.extend(
            SourceDiagnostic(
                phase="validation",
                condition="odm_schema_field_type",
                spec_paths=path,
                requirement="REQ-1275",
                context={
                    "dataset": dataset,
                    "field": field,
                    "stored_field": stored,
                    "stored_type": stored_types[stored] or "unsupported",
                },
            )
            for field, stored in binding.stored.items()
            if stored_types[stored] != "str"
        )
    return binding.stored, diagnostics


def _odm_record_diagnostics(
    dataset: str,
    table: TypedTable,
    first_record: int,
) -> list[SourceDiagnostic]:
    """Report the first record lacking each identifier REQ-1268 requires."""
    from yamaa.odm.items import ODM_REQUIRED_VALUES

    diagnostics: list[SourceDiagnostic] = []
    for field in ODM_REQUIRED_VALUES:
        lacking = table.frame.with_row_index("record", offset=first_record).filter(
            pl.col(field).is_null()
        )
        if lacking.height:
            diagnostics.append(
                SourceDiagnostic(
                    phase="ingest",
                    condition="odm_schema_value_missing",
                    spec_paths=(f"input.{dataset}.path",),
                    requirement="REQ-1276",
                    context={
                        "dataset": dataset,
                        "field": field,
                        "record": int(lacking["record"][0]),
                        "records": lacking.height,
                    },
                )
            )
    return diagnostics


def _odm_table(
    dataset: str,
    source: DatasetSource,
    content: bytes,
    profile: str,
) -> TypedTable:
    """Read an ODM input under its fixed schema (REQ-1266 through REQ-1268).

    Only the schema fields are read, under the schema's names and as `str`;
    a vendor field is neither typed nor exposed (REQ-1267), which is also
    what lets a Parquet file carry fields the closed mapping cannot type.
    """
    from yamaa.odm.items import ODM_SCHEMA_FIELDS

    if profile == "csv":
        parsed = parse_csv(content)
        bound, diagnostics = _odm_field_diagnostics(dataset, parsed.names, None)
        if diagnostics:
            raise SourceError(diagnostics)
        positions = [parsed.names.index(bound[field]) for field in ODM_SCHEMA_FIELDS]
        projected = CsvSource(
            names=ODM_SCHEMA_FIELDS,
            records=tuple(
                tuple(record[position] for position in positions)
                for record in parsed.records
            ),
        )
        # CSV records are numbered from the header, which is record one.
        table = _build_table(dataset, DatasetSource(path=source.path), projected)
        first_record = 2
    else:
        arrow, fields = parquet_source_fields(content)
        bound, diagnostics = _odm_field_diagnostics(
            dataset, [name for name, _ in fields], dict(fields)
        )
        if diagnostics:
            raise SourceError(diagnostics)
        table = parquet_text_columns(
            arrow, [(bound[field], field) for field in ODM_SCHEMA_FIELDS]
        )
        if source.empty_string == "missing":
            table = _empty_strings_to_missing(table)
        first_record = 1
    records = _odm_record_diagnostics(dataset, table, first_record)
    if records:
        raise SourceError(records)
    return table


def _odm_declared_diagnostic(
    dataset: str, source: DatasetSource
) -> SourceDiagnostic | None:
    """Reject the `types` or `schema` an ODM input declares (REQ-1268)."""
    if source.types is None and source.schema_path is None:
        return None
    declared = "types" if source.types is not None else "schema"
    return SourceDiagnostic(
        phase="validation",
        condition="odm_schema_field_type",
        spec_paths=(f"input.{dataset}.{declared}",),
        requirement="REQ-1275",
        context={"dataset": dataset, "declared": declared},
    )


def odm_schema_sources(
    datasets: Mapping[str, DatasetSource],
    sources: Mapping[str, LoadedDataset | TypedTable],
    odm_datasets: Collection[str],
) -> dict[str, LoadedDataset | TypedTable]:
    """Hold every ODM input a caller supplied to its fixed schema.

    Ingestion given ``odm_datasets`` has already read each ODM input under
    the schema, and that table passes unchanged. A table read any other way,
    or handed over directly, is bound, verified, and projected to the schema
    here, so no `odm` read reaches an unverified input whichever provider
    supplied it (REQ-1268).
    """
    from yamaa.odm.items import ODM_SCHEMA_FIELDS

    held = dict(sources)
    diagnostics: list[SourceDiagnostic] = []
    for dataset in sorted(set(odm_datasets) & set(sources)):
        declared = _odm_declared_diagnostic(dataset, datasets[dataset])
        if declared is not None:
            diagnostics.append(declared)
            continue
        supplied = sources[dataset]
        table = supplied.table if isinstance(supplied, LoadedDataset) else supplied
        bound, fields = _odm_field_diagnostics(
            dataset,
            [column.name for column in table.columns],
            {column.name: column.type for column in table.columns},
        )
        if fields:
            diagnostics.extend(fields)
            continue
        if tuple(column.name for column in table.columns) != ODM_SCHEMA_FIELDS:
            table = TypedTable(
                columns=tuple(
                    TypedColumn(name=field, type="str") for field in ODM_SCHEMA_FIELDS
                ),
                frame=table.frame.select(
                    pl.col(bound[field]).alias(field) for field in ODM_SCHEMA_FIELDS
                ),
            )
        # CSV records are numbered from the header, which is record one.
        first_record = (
            2
            if isinstance(supplied, LoadedDataset)
            and profile_of(supplied.written_path) == "csv"
            else 1
        )
        records = _odm_record_diagnostics(dataset, table, first_record)
        if records:
            diagnostics.extend(records)
            continue
        held[dataset] = (
            supplied.model_copy(update={"table": table})
            if isinstance(supplied, LoadedDataset)
            else table
        )
    if diagnostics:
        raise SourceError(diagnostics)
    return held


def load_source_tables(
    datasets: Mapping[str, DatasetSource],
    resources: ProjectResources,
    *,
    producer_contracts: Mapping[str, ProducerContract] | None = None,
    producer_snapshots: Mapping[str, ResourceSnapshot] | None = None,
    origins: Mapping[str, tuple[Path, str]] | None = None,
    odm_datasets: Collection[str] = (),
) -> dict[str, LoadedDataset]:
    """Capture, verify, and ingest normalized dataset declarations.

    ``origins`` maps a dataset to the directory of the layer that wrote its
    path and that layer's own spelling. REQ-0780 resolves the path from
    there and retries that spelling from the project root and data roots;
    a dataset without an origin resolves ``source.path`` from ``resources``.
    ``odm_datasets`` names the ODM inputs, which are read under the fixed
    ODM schema rather than the generic field typing (REQ-1268).
    """
    contracts = producer_contracts or {}
    snapshots = producer_snapshots or {}
    views: dict[str, tuple[ProjectResources, str]] = {}
    for dataset, source in datasets.items():
        origin = (origins or {}).get(dataset)
        views[dataset] = (
            (resources.with_base_directory(origin[0]), origin[1])
            if origin is not None
            else (resources, source.path)
        )
    diagnostics: list[SourceDiagnostic] = []
    for dataset, source in datasets.items():
        view, written = views[dataset]
        profile = profile_of(source.path)
        declared = (
            _odm_declared_diagnostic(dataset, source)
            if dataset in odm_datasets
            else None
        )
        if declared is not None:
            # REQ-1268: an ODM input's types are its schema's.
            diagnostics.append(declared)
            continue
        if source.schema_path is not None and source.types is not None:
            diagnostics.extend(
                SourceDiagnostic(
                    phase="validation",
                    condition="redundant_field_type",
                    spec_paths=(f"input.{dataset}.types.{field}",),
                    requirement="REQ-0523",
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
                    requirement="REQ-0533",
                    context={
                        "dataset": dataset,
                        "field": field,
                        "type": value,
                    },
                )
                for field, value in source.types.items()
            )
        if profile == "csv" and source.empty_string == "present":
            diagnostics.append(
                SourceDiagnostic(
                    phase="validation",
                    condition="empty_string_present_unsupported",
                    spec_paths=(f"input.{dataset}.empty_string",),
                    requirement="REQ-1161",
                    context={"dataset": dataset, "path": source.path},
                )
            )
        try:
            if dataset in snapshots:
                view.validate_location(written)
            else:
                view.validate(written)
            if profile is None:
                diagnostics.append(_profile_diagnostic(dataset, source.path))
        except ResourceFailure as failure:
            diagnostics.append(
                _path_diagnostic(
                    dataset,
                    source.path,
                    failure,
                    data_roots_checked=view.fallback_root_count(written),
                )
            )
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
        view, written = views[dataset]
        try:
            injected = snapshots.get(dataset)
            snapshot = injected or view.capture(written)
            if injected is None and id(snapshot) not in seen:
                view.verify(snapshot)
            seen.add(id(snapshot))
            profile = profile_of(source.path)
            assert profile is not None
            if dataset in odm_datasets:
                table = _odm_table(dataset, source, snapshot.content, profile)
            elif profile == "csv":
                parsed = parse_csv(snapshot.content)
                table = _build_table(dataset, source, parsed, contracts.get(dataset))
            else:
                table = _validate_parquet_contract(
                    dataset,
                    parse_parquet(snapshot.content),
                    contracts.get(dataset),
                )
                if source.empty_string == "missing":
                    table = _empty_strings_to_missing(table)
        except ResourceFailure as failure:
            diagnostics.append(
                _path_diagnostic(
                    dataset,
                    source.path,
                    failure,
                    data_roots_checked=view.fallback_root_count(written),
                )
            )
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
