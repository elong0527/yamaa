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
    datasets: dict[str, DatasetSource]
    base: str | None = None
    parents: list[str] | None = None
    record_lookups: list[RecordLookup] | None = None
    keys: list[str]
    output: Output
    columns: list[Column]
    rows: list[Row] | None = None
    verifications: list[Expression] | None = None
    metadata: dict[str, str] | None = None


class LoadedSpecification(_StrictModel):
    """A normalized specification and its file origins."""

    specification: Specification
    written_path: Path
    origin_path: Path
    schema_path: Path
