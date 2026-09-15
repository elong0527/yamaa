"""Strict Pydantic models for normalized YAMAA specifications."""

from __future__ import annotations

from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue, RootModel, model_validator

ColumnType = Literal["str", "int", "float", "date", "datetime"]


class _StrictModel(BaseModel):
    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)


class Expression(RootModel[dict[str, JsonValue]]):
    """One normalized public expression operation."""

    model_config = ConfigDict(strict=True, frozen=True)

    @model_validator(mode="after")
    def require_one_operation(self) -> Expression:
        if len(self.root) != 1:
            raise ValueError("an expression must contain exactly one operation")
        return self

    @property
    def operation(self) -> str:
        return next(iter(self.root))


class DatasetSource(_StrictModel):
    path: str
    types: dict[str, ColumnType] | None = None
    schema_path: str | None = Field(default=None, alias="schema")


class OrderTerm(_StrictModel):
    variable: str
    direction: Literal["asc", "desc"] = "asc"
    nulls: Literal["first", "last"] = "last"


class Output(_StrictModel):
    path: str
    decimals: int | None = None
    columns: list[str]
    violation_log: str | None = None
    order_by: list[OrderTerm] | None = None


class RecordLookupBetween(_StrictModel):
    value: str
    lower: str
    upper: str


class RecordLookup(_StrictModel):
    id: str
    dataset: str
    source: list[str] | None = None
    key: list[str] | None = None
    between: RecordLookupBetween | None = None
    filter: str | None = None
    order_by: list[OrderTerm] | None = None
    keep: Literal["first", "last"] | None = None
    unmatched: Literal["missing", "fail"] | None = None
    incomplete: Literal["missing", "fail"] | None = None


class OverrideRule(_StrictModel):
    when: str
    value: Expression


class HandledExpression(_StrictModel):
    value: Expression
    conversion_failure: JsonValue = None
    override: list[OverrideRule] | None = None


class Column(_StrictModel):
    name: str
    type: ColumnType
    label: str | None = None
    derivation: HandledExpression | None = None
    verifications: list[Expression] | None = None
    metadata: dict[str, str] | None = None


class Row(_StrictModel):
    id: str
    dataset: str | None = None
    group_by: list[str] | None = None
    filter: str | None = None
    derivations: dict[str, HandledExpression]


class Specification(_StrictModel):
    schema_version: str
    domain: str
    datasets: dict[str, DatasetSource] | None = None
    input: dict[str, DatasetSource] | None = None
    base: str | None = None
    parents: list[str] | None = None
    record_lookups: list[RecordLookup] | None = None
    keys: list[str]
    output: Output
    columns: list[Column]
    rows: list[Row] | None = None
    verifications: list[Expression] | None = None
    metadata: dict[str, str] | None = None

    @model_validator(mode="before")
    @classmethod
    def _normalize_input_alias(cls, data: object) -> object:
        if isinstance(data, dict):
            has_datasets = data.get("datasets") is not None
            has_input = data.get("input") is not None
            if has_datasets == has_input:
                raise ValueError(
                    "exactly one of 'datasets' or 'input' must be present"
                )
        return _normalize_input_alias_in_dict(data)

    @property
    def default_driver(self):
        datasets = self.datasets
        if datasets is None:
            return None
        if self.base is not None:
            return self.base
        if len(datasets) == 1:
            return next(iter(datasets))
        return None

    @property
    def is_new_style(self) -> bool:
        """New-style specs derive every key from a column.

        The gate is shape-only: every top-level key column must have a
        non-None derivation.  ``input:`` and ``datasets:`` are aliases with
        identical semantics; the spelling does not affect new-vs-legacy.
        """
        derivations = {
            column.name: column.derivation
            for column in self.columns
            if column.derivation is not None
        }
        return all(derivations.get(key) is not None for key in self.keys)


def _normalize_input_alias_in_dict(data: object) -> object:
    """Rename 'input' to 'datasets' without enforcing exactly-one.

    Parent layers may omit both aliases, and the fully resolved spec is
    validated once by the Specification model validator.
    """
    if not isinstance(data, dict):
        return data
    if data.get("input") is not None:
        data = dict(data)
        data["datasets"] = data.pop("input")
    return data


class LoadedSpecification(_StrictModel):
    """A normalized specification and its file origins."""

    specification: Specification
    written_path: Path
    origin_path: Path
    schema_path: Path
