"""Plan R002 source bindings from normalized declarations and typed tables."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Literal, TypeAlias

from pydantic import BaseModel, ConfigDict, Field, model_validator

from yamaa.io.source import LoadedDataset
from yamaa.models import RuntimeCondition, TypedColumn, TypedTable
from yamaa.specification.models import Specification

ODM_CONTEXT_COLUMNS = (
    "StudyOID",
    "MetaDataVersionOID",
    "SubjectKey",
    "StudyEventOID",
    "StudyEventRepeatKey",
    "FormOID",
    "FormRepeatKey",
    "ItemGroupOID",
    "ItemGroupRepeatKey",
)


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class DatasetBinding(_FrozenModel):
    """The ordered fields and ODM context carried by one source relation."""

    dataset: str = Field(min_length=1)
    columns: tuple[TypedColumn, ...]
    context_columns: tuple[str, ...]

    @property
    def field_names(self) -> tuple[str, ...]:
        return tuple(column.name for column in self.columns)

    @property
    def is_long_form_odm(self) -> bool:
        fields = self.field_names
        return "ItemOID" in fields and "Value" in fields


class BoundReference(_FrozenModel):
    """One source name classified before row-level resolution."""

    name: str = Field(min_length=1)
    kind: Literal["output", "dataset", "odm_item"]
    dataset: str | None = None
    field: str | None = None
    item_oid: str | None = None
    context_columns: tuple[str, ...] = ()

    @model_validator(mode="after")
    def validate_shape(self) -> BoundReference:
        if self.kind == "output" and (
            self.dataset is not None
            or self.field is None
            or self.item_oid is not None
            or self.context_columns
        ):
            raise ValueError("an output binding carries only its field")
        if self.kind == "dataset" and (
            self.dataset is None
            or self.field is None
            or self.item_oid is not None
            or self.context_columns
        ):
            raise ValueError("a dataset binding requires a dataset and field")
        if self.kind == "odm_item" and (
            self.dataset is None
            or self.field is not None
            or self.item_oid is None
            or not self.context_columns
        ):
            raise ValueError("an ODM item binding requires its complete context")
        return self


class BindingFailure(_FrozenModel):
    """A source name that cannot be bound under R002."""

    condition: RuntimeCondition


BindingResult: TypeAlias = BoundReference | BindingFailure


def _unknown(name: str) -> BindingFailure:
    return BindingFailure(
        condition=RuntimeCondition(
            phase="validation",
            condition="unknown_field",
            context={"identifier": name},
        )
    )


class BindingPlan(_FrozenModel):
    """Names visible to row contexts for one normalized specification."""

    domain: str = Field(min_length=1)
    datasets: dict[str, DatasetBinding]
    output_columns: tuple[str, ...]

    def bind(self, name: str) -> BindingResult:
        """Classify one complete variable name without reading a row."""
        if not name or "." not in name:
            if name in self.output_columns:
                return BoundReference(name=name, kind="output", field=name)
            return _unknown(name)

        dataset_name, field = name.split(".", 1)
        dataset = self.datasets.get(dataset_name)
        if dataset is None:
            return _unknown(name)
        if field in dataset.field_names:
            return BoundReference(
                name=name,
                kind="dataset",
                dataset=dataset_name,
                field=field,
            )

        # Dataset fields take precedence above. Every other suffix on a
        # long-form ODM relation is a complete ItemOID; R002-20 permits, but
        # does not require, periods inside that identifier.
        if dataset.is_long_form_odm:
            if not dataset.context_columns:
                return _unknown(name)
            return BoundReference(
                name=name,
                kind="odm_item",
                dataset=dataset_name,
                item_oid=field,
                context_columns=dataset.context_columns,
            )
        return _unknown(name)


def _typed_table(value: LoadedDataset | TypedTable) -> TypedTable:
    return value.table if isinstance(value, LoadedDataset) else value


def build_binding_plan(
    specification: Specification,
    sources: Mapping[str, LoadedDataset | TypedTable],
) -> BindingPlan:
    """Build the static binding catalog for loaded normalized sources."""
    declared = tuple(specification.datasets)
    supplied = tuple(sources)
    if set(declared) != set(supplied):
        raise ValueError(
            "loaded source names must exactly match normalized dataset declarations"
        )

    datasets: dict[str, DatasetBinding] = {}
    for dataset_name in declared:
        table = _typed_table(sources[dataset_name])
        fields = tuple(column.name for column in table.columns)
        datasets[dataset_name] = DatasetBinding(
            dataset=dataset_name,
            columns=table.columns,
            context_columns=tuple(
                column for column in ODM_CONTEXT_COLUMNS if column in fields
            ),
        )

    return BindingPlan(
        domain=specification.domain,
        datasets=datasets,
        output_columns=tuple(column.name for column in specification.columns),
    )
