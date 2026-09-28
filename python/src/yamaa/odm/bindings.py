"""Plan R002 source bindings from normalized declarations and typed tables."""

from __future__ import annotations

from collections.abc import Mapping
from typing import Literal, TypeAlias

from pydantic import BaseModel, ConfigDict, Field, model_validator

from yamaa.io.source import LoadedDataset
from yamaa.models import RuntimeCondition, TypedColumn, TypedTable
from yamaa.specification.models import Specification


class _FrozenModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class DatasetBinding(_FrozenModel):
    """The ordered fields carried by one source relation."""

    dataset: str = Field(min_length=1)
    columns: tuple[TypedColumn, ...]

    @property
    def field_names(self) -> tuple[str, ...]:
        return tuple(column.name for column in self.columns)


class BoundReference(_FrozenModel):
    """One source name classified before row-level resolution."""

    name: str = Field(min_length=1)
    kind: Literal["output", "dataset"]
    dataset: str | None = None
    field: str | None = None

    @model_validator(mode="after")
    def validate_shape(self) -> BoundReference:
        if self.kind == "output" and (self.dataset is not None or self.field is None):
            raise ValueError("an output binding carries only its field")
        if self.kind == "dataset" and (self.dataset is None or self.field is None):
            raise ValueError("a dataset binding requires a dataset and field")
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
        # An ODM item is read with `odm` (REQ-1265), never through a
        # variable name, so any other suffix names no field.
        return _unknown(name)


def _typed_table(value: LoadedDataset | TypedTable) -> TypedTable:
    return value.table if isinstance(value, LoadedDataset) else value


def build_binding_plan(
    specification: Specification,
    sources: Mapping[str, LoadedDataset | TypedTable],
) -> BindingPlan:
    """Build the static binding catalog for loaded normalized sources."""
    declared = tuple(specification.input)
    supplied = tuple(sources)
    if set(declared) != set(supplied):
        raise ValueError(
            "loaded source names must exactly match normalized dataset declarations"
        )

    datasets: dict[str, DatasetBinding] = {}
    for dataset_name in declared:
        datasets[dataset_name] = DatasetBinding(
            dataset=dataset_name,
            columns=_typed_table(sources[dataset_name]).columns,
        )

    return BindingPlan(
        domain=specification.domain,
        datasets=datasets,
        output_columns=tuple(column.name for column in specification.columns),
    )
