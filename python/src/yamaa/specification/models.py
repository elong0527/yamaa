"""Strict Pydantic models for normalized yamaa specifications."""

from __future__ import annotations

from pathlib import Path
from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue, RootModel, model_validator

ColumnType = Literal["str", "int", "float", "date", "datetime"]

DatasetClassName = Literal[
    "ADAM OTHER",
    "BASIC DATA STRUCTURE",
    "DEVICE LEVEL ANALYSIS DATASET",
    "EVENTS",
    "FINDINGS",
    "FINDINGS ABOUT",
    "INTERVENTIONS",
    "MEDICAL DEVICE BASIC DATA STRUCTURE",
    "MEDICAL DEVICE OCCURRENCE DATA STRUCTURE",
    "OCCURRENCE DATA STRUCTURE",
    "REFERENCE DATA STRUCTURE",
    "RELATIONSHIP",
    "SPECIAL PURPOSE",
    "STUDY REFERENCE",
    "SUBJECT LEVEL ANALYSIS DATASET",
    "TRIAL DESIGN",
]

DatasetSubclassName = Literal[
    "ADVERSE EVENT",
    "MEDICAL DEVICE TIME-TO-EVENT",
    "NON-COMPARTMENTAL ANALYSIS",
    "POPULATION PHARMACOKINETIC ANALYSIS",
    "TIME-TO-EVENT",
]

CoreDesignation = Literal["Req", "Exp", "Perm"]

OriginType = Literal[
    "Assigned",
    "Collected",
    "Derived",
    "Not Available",
    "Other",
    "Predecessor",
    "Protocol",
]

OriginSource = Literal["Investigator", "Sponsor", "Subject", "Vendor"]

MethodType = Literal["Computation", "Imputation"]

DefineDataType = Literal[
    "text",
    "integer",
    "float",
    "date",
    "datetime",
    "time",
    "partialDate",
    "partialTime",
    "partialDatetime",
    "incompleteDate",
    "incompleteTime",
    "incompleteDatetime",
    "durationDatetime",
    "intervalDatetime",
    "URI",
]

PageRefType = Literal["PhysicalRef", "NamedDestination"]


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
    # REQ-1158: how a stored empty string in a character field reads.
    empty_string: Literal["missing", "present"] = "missing"


class OrderTerm(_StrictModel):
    variable: str
    direction: Literal["asc", "desc"] = "asc"
    nulls: Literal["first", "last"] = "last"


class Output(_StrictModel):
    path: str
    decimals: int | None = None
    columns: list[str]
    violation_log: str | None = None
    verification_report: str | None = None
    order_by: list[OrderTerm] | None = None


class IntermediateBetween(_StrictModel):
    value: str
    lower: str
    upper: str


class Intermediate(_StrictModel):
    id: str
    dataset: str
    key_base: list[str] | None = None
    key: list[str] | None = None
    between: IntermediateBetween | None = None
    filter: str | None = None
    order_by: list[OrderTerm] | None = None
    keep: Literal["first", "last"] | None = None
    columns: list[str] | None = None
    missing: JsonValue = None
    strict: bool = False


class HandledExpression(_StrictModel):
    value: Expression
    conversion_failure: JsonValue = None


class PageReference(_StrictModel):
    type: PageRefType
    refs: str
    title: str | None = None


class DocumentReferenceClass(_StrictModel):
    document: str
    pages: PageReference | None = None


DocumentReference = str | DocumentReferenceClass


class FormalExpression(_StrictModel):
    context: str
    code: str


class SubmissionMethodClass(_StrictModel):
    name: str | None = None
    type: MethodType = "Computation"
    description: str
    expression: FormalExpression | None = None
    documents: list[DocumentReference] | None = None


SubmissionMethod = str | SubmissionMethodClass


class SubmissionCommentClass(_StrictModel):
    text: str
    documents: list[DocumentReference] | None = None


SubmissionComment = str | SubmissionCommentClass


class SubmissionOrigin(_StrictModel):
    type: OriginType
    source: OriginSource | None = None
    description: str | None = None
    documents: list[DocumentReference] | None = None


class SubmissionColumn(_StrictModel):
    core: CoreDesignation | None = None
    mandatory: bool | None = None
    role: str | None = None
    data_type: DefineDataType | None = None
    length: int | None = None
    significant_digits: int | None = None
    display_format: str | None = None
    codelist: str | None = None
    origin: SubmissionOrigin
    method: SubmissionMethod | None = None
    comment: SubmissionComment | None = None


class SubmissionDataset(_StrictModel):
    label: str
    class_name: DatasetClassName = Field(alias="class")
    subclass: DatasetSubclassName | None = None
    structure: str
    repeating: bool
    reference_data: bool = False
    domain: str | None = None
    comment: SubmissionComment | None = None


class Column(_StrictModel):
    name: str
    type: ColumnType
    label: str | None = None
    derivation: HandledExpression | None = None
    verifications: list[Expression] | None = None
    submission: SubmissionColumn | None = None
    metadata: dict[str, str] | None = None


class Row(_StrictModel):
    id: str
    dataset: str | None = None
    group_by: list[str] | None = None
    filter: str | None = None
    derivations: dict[str, HandledExpression]
    submission: dict[str, SubmissionColumn] | None = None


class Specification(_StrictModel):
    schema_version: str
    domain: str
    input: dict[str, DatasetSource]
    base: str | None = None
    parents: list[str] | None = None
    intermediates: list[Intermediate] | None = None
    keys: list[str]
    output: Output
    columns: list[Column]
    rows: list[Row] | None = None
    filter: str | None = None
    verifications: list[Expression] | None = None
    submission: SubmissionDataset | None = None
    metadata: dict[str, str] | None = None

    @property
    def default_driver(self):
        if self.base is not None:
            return self.base
        if len(self.input) == 1:
            return next(iter(self.input))
        return None


class LoadedSpecification(_StrictModel):
    """A normalized specification and its file origins."""

    specification: Specification
    written_path: Path
    origin_path: Path
    schema_path: Path
