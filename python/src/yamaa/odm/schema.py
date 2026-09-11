"""Pydantic and Polars contracts for the canonical clinical-item dataset."""

from collections.abc import Iterable
from pathlib import Path

import polars as pl
from pydantic import BaseModel, ConfigDict, Field, model_validator

SCHEMA_VERSION = "odm-clinical-item/1"

ODM_ITEM_SCHEMA = pl.Schema(
    {
        "ODMVersion": pl.String,
        "FileOID": pl.String,
        "StudyOID": pl.String,
        "MetaDataVersionOID": pl.String,
        "SubjectKey": pl.String,
        "StudySubjectID": pl.String,
        "SubjectStatus": pl.String,
        "StudyEventOID": pl.String,
        "StudyEventRepeatKey": pl.String,
        "EventName": pl.String,
        "StartDate": pl.String,
        "EventStatus": pl.String,
        "EventWorkflowStatus": pl.String,
        "FormOID": pl.String,
        "FormRepeatKey": pl.String,
        "FormName": pl.String,
        "FormLayoutOID": pl.String,
        "FormStatus": pl.String,
        "FormWorkflowStatus": pl.String,
        "OpenQueries": pl.String,
        "ItemGroupOID": pl.String,
        "ItemGroupRepeatKey": pl.String,
        "ItemGroupName": pl.String,
        "TransactionType": pl.String,
        "ItemOID": pl.String,
        "ItemName": pl.String,
        "Value": pl.String,
        "ValuePresent": pl.Boolean,
        "IsNull": pl.Boolean,
        "SourceOrdinal": pl.UInt64,
    }
)


class ClinicalItemRow(BaseModel):
    """One lossless clinical item and its complete projected context."""

    model_config = ConfigDict(
        strict=True,
        extra="forbid",
        populate_by_name=True,
        frozen=True,
    )

    odm_version: str = Field(alias="ODMVersion", min_length=1)
    file_oid: str | None = Field(alias="FileOID")
    study_oid: str = Field(alias="StudyOID", min_length=1)
    metadata_version_oid: str = Field(alias="MetaDataVersionOID", min_length=1)
    subject_key: str = Field(alias="SubjectKey", min_length=1)
    study_subject_id: str | None = Field(alias="StudySubjectID")
    subject_status: str | None = Field(alias="SubjectStatus")
    study_event_oid: str = Field(alias="StudyEventOID", min_length=1)
    study_event_repeat_key: str | None = Field(alias="StudyEventRepeatKey")
    event_name: str | None = Field(alias="EventName")
    start_date: str | None = Field(alias="StartDate")
    event_status: str | None = Field(alias="EventStatus")
    event_workflow_status: str | None = Field(alias="EventWorkflowStatus")
    form_oid: str = Field(alias="FormOID", min_length=1)
    form_repeat_key: str | None = Field(alias="FormRepeatKey")
    form_name: str | None = Field(alias="FormName")
    form_layout_oid: str | None = Field(alias="FormLayoutOID")
    form_status: str | None = Field(alias="FormStatus")
    form_workflow_status: str | None = Field(alias="FormWorkflowStatus")
    open_queries: str | None = Field(alias="OpenQueries")
    item_group_oid: str = Field(alias="ItemGroupOID", min_length=1)
    item_group_repeat_key: str | None = Field(alias="ItemGroupRepeatKey")
    item_group_name: str | None = Field(alias="ItemGroupName")
    transaction_type: str | None = Field(alias="TransactionType")
    item_oid: str = Field(alias="ItemOID", min_length=1)
    item_name: str | None = Field(alias="ItemName")
    value: str | None = Field(alias="Value")
    value_present: bool = Field(alias="ValuePresent")
    is_null: bool = Field(alias="IsNull")
    source_ordinal: int = Field(alias="SourceOrdinal", ge=1)

    @model_validator(mode="after")
    def validate_value_state(self) -> "ClinicalItemRow":
        """Keep absent, explicit-null, empty, and ordinary values distinct."""
        if self.is_null and self.value_present:
            raise ValueError("an explicitly null item cannot contain a value")
        if self.value_present and self.value is None:
            raise ValueError("a present value must contain text; empty text is ''")
        if not self.value_present and self.value is not None:
            raise ValueError("an absent value must use Value=None")
        return self

    def polars_row(self) -> dict[str, object]:
        """Return a row using the public Parquet column names."""
        return self.model_dump(by_alias=True, mode="python")


def rows_to_frame(records: Iterable[ClinicalItemRow]) -> pl.DataFrame:
    """Create a fixed-schema Polars frame from validated clinical-item rows."""
    return pl.DataFrame(
        [record.polars_row() for record in records],
        schema=ODM_ITEM_SCHEMA,
        orient="row",
        strict=True,
    ).select(ODM_ITEM_SCHEMA.names())


class ParquetWriteResult(BaseModel):
    """Result returned after atomically publishing one Parquet file."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    output_path: Path
    row_count: int = Field(ge=0)
