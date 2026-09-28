from __future__ import annotations

from pathlib import Path

import pyarrow as pa
import pyarrow.parquet as pq
import pytest

from yamaa.io import ProjectResources, SourceError, load_source_tables
from yamaa.io.polars import frame_from_values
from yamaa.io.source import odm_schema_sources
from yamaa.models import TypedColumn
from yamaa.odm.items import (
    ODM_SCHEMA_FIELDS,
    OdmRead,
    bind_fields,
    fold_name,
    parse_odm_read,
    schema_field,
)
from yamaa.odm.parquet import write_odm_parquet
from yamaa.specification.models import DatasetSource

HEADER = ",".join(ODM_SCHEMA_FIELDS)
RECORD = "S1,M1,001,SCREENING,1,FO.DM,1,IG.DM,1,IT.DM.AGE,34"


def _diagnostics(error: SourceError) -> list[dict[str, object]]:
    return [
        {
            "condition": diagnostic.condition,
            "spec_paths": list(diagnostic.spec_paths),
            "context": diagnostic.context,
        }
        for diagnostic in error.diagnostics
    ]


def _load(tmp_path: Path, name: str, content: bytes, **declared: object):
    (tmp_path / name).write_bytes(content)
    return load_source_tables(
        {"ODM": DatasetSource(path=name, **declared)},
        ProjectResources(tmp_path),
        odm_datasets={"ODM"},
    )["ODM"]


def test_names_fold_only_ascii_letters() -> None:
    assert fold_name("ItemGroupRepeatKey") == "itemgrouprepeatkey"
    assert schema_field("STUDYOID") == "StudyOID"
    assert schema_field("itemoid") == "ItemOID"
    # REQ-1267: U+0130 folds to itself, so it binds to nothing.
    assert schema_field("\u0130temOID") is None
    assert schema_field("SITEID") is None


def test_fields_bind_once_each_and_report_what_is_missing_or_ambiguous() -> None:
    binding = bind_fields(["STUDYOID", "Value", "VALUE", "SITEID"])

    assert binding.stored == {"StudyOID": "STUDYOID"}
    assert binding.ambiguous == {"Value": ("Value", "VALUE")}
    assert "Value" not in binding.missing
    assert binding.missing == tuple(
        field for field in ODM_SCHEMA_FIELDS if field not in ("StudyOID", "Value")
    )


def test_an_item_keeps_every_period_after_the_qualifier() -> None:
    assert parse_odm_read("ODM.IT.LB.LBDTC") == OdmRead(
        dataset="ODM",
        item_oid="IT.LB.LBDTC",
        events=None,
        forms=None,
        item_groups=None,
        filter=None,
    )
    assert parse_odm_read(
        {
            "item": "ODM.AGE",
            "event": "SCREENING",
            "form": ["FO.A", "FO.B"],
            "filter": "ODM.ItemGroupRepeatKey = '1'",
        }
    ) == OdmRead(
        dataset="ODM",
        item_oid="AGE",
        events=("SCREENING",),
        forms=("FO.A", "FO.B"),
        item_groups=None,
        filter="ODM.ItemGroupRepeatKey = '1'",
    )
    assert parse_odm_read("NOPERIOD") is None


def test_a_csv_input_is_read_under_the_schema_names_only(tmp_path: Path) -> None:
    header = HEADER.upper() + ",SITEID"
    loaded = _load(tmp_path, "odm.csv", f"{header}\n{RECORD},SITE-9\n".encode())

    assert loaded.table.columns == tuple(
        TypedColumn(name=field, type="str") for field in ODM_SCHEMA_FIELDS
    )
    assert loaded.table.frame.row(0) == tuple(RECORD.split(","))


@pytest.mark.parametrize(
    ("header", "context"),
    [
        (
            HEADER.replace("FormOID,FormRepeatKey,", ""),
            {"dataset": "ODM", "fields": ["FormOID", "FormRepeatKey"]},
        ),
        (
            HEADER + ",VALUE",
            {"dataset": "ODM", "field": "Value", "stored": ["Value", "VALUE"]},
        ),
    ],
)
def test_a_csv_header_is_verified_before_any_record(
    tmp_path: Path, header: str, context: dict[str, object]
) -> None:
    with pytest.raises(SourceError) as raised:
        _load(tmp_path, "odm.csv", f"{header}\n".encode())

    [diagnostic] = _diagnostics(raised.value)
    assert diagnostic["spec_paths"] == ["input.ODM.path"]
    assert diagnostic["context"] == context


def test_a_record_without_an_identifier_names_its_csv_record(tmp_path: Path) -> None:
    lacking = RECORD.replace("001", "")
    content = f"{HEADER}\n{RECORD}\n{lacking}\n{lacking}\n".encode()

    with pytest.raises(SourceError) as raised:
        _load(tmp_path, "odm.csv", content)

    assert _diagnostics(raised.value) == [
        {
            "condition": "odm_schema_value_missing",
            "spec_paths": ["input.ODM.path"],
            "context": {
                "dataset": "ODM",
                "field": "SubjectKey",
                "record": 3,
                "records": 2,
            },
        }
    ]


def test_an_odm_input_declares_no_types(tmp_path: Path) -> None:
    with pytest.raises(SourceError) as raised:
        _load(
            tmp_path,
            "odm.csv",
            f"{HEADER}\n{RECORD}\n".encode(),
            types={"Value": "int"},
        )

    assert _diagnostics(raised.value) == [
        {
            "condition": "odm_schema_field_type",
            "spec_paths": ["input.ODM.types"],
            "context": {"dataset": "ODM", "declared": "types"},
        }
    ]


def _parquet(tmp_path: Path, columns: dict[str, pa.Array]) -> bytes:
    path = tmp_path / "odm.parquet"
    pq.write_table(pa.table(columns), path)
    return path.read_bytes()


def test_a_parquet_vendor_field_is_never_typed(tmp_path: Path) -> None:
    columns = {
        field.upper(): pa.array([value], pa.large_string())
        for field, value in zip(ODM_SCHEMA_FIELDS, RECORD.split(","), strict=True)
    }
    # A type the closed mapping cannot read, on a field the schema ignores.
    columns["VISITS"] = pa.array([[1, 2]], pa.list_(pa.int32()))
    loaded = _load(tmp_path, "input.parquet", _parquet(tmp_path, columns))

    assert [column.name for column in loaded.table.columns] == list(ODM_SCHEMA_FIELDS)
    assert loaded.table.frame.row(0) == tuple(RECORD.split(","))


def test_a_parquet_schema_field_stored_as_a_number_fails(tmp_path: Path) -> None:
    columns = {
        field: pa.array([value])
        for field, value in zip(ODM_SCHEMA_FIELDS, RECORD.split(","), strict=True)
    }
    columns["Value"] = pa.array([34], pa.int64())

    with pytest.raises(SourceError) as raised:
        _load(tmp_path, "input.parquet", _parquet(tmp_path, columns))

    assert _diagnostics(raised.value) == [
        {
            "condition": "odm_schema_field_type",
            "spec_paths": ["input.ODM.path"],
            "context": {
                "dataset": "ODM",
                "field": "Value",
                "stored_field": "Value",
                "stored_type": "int",
            },
        }
    ]


def test_the_odm_parquet_writer_output_verifies(
    odm13_path: Path, tmp_path: Path
) -> None:
    write_odm_parquet(odm13_path, tmp_path / "items.parquet")

    loaded = load_source_tables(
        {"ODM": DatasetSource(path="items.parquet")},
        ProjectResources(tmp_path),
        odm_datasets={"ODM"},
    )["ODM"]

    assert [column.name for column in loaded.table.columns] == list(ODM_SCHEMA_FIELDS)
    assert loaded.table.frame["ItemOID"].to_list() == ["I.DATE", "I.EMPTY", "I.NULL"]


def test_a_table_supplied_without_the_schema_read_is_held_to_it() -> None:
    names = [field.lower() for field in ODM_SCHEMA_FIELDS] + ["SITEID"]
    table = frame_from_values(
        tuple(TypedColumn(name=name, type="str") for name in names),
        [[*RECORD.split(","), "SITE-9"]],
    )
    declared = {"ODM": DatasetSource(path="odm.csv")}

    held = odm_schema_sources(declared, {"ODM": table}, {"ODM"})["ODM"]

    assert [column.name for column in held.columns] == list(ODM_SCHEMA_FIELDS)
    assert held.frame.row(0) == tuple(RECORD.split(","))


def test_a_supplied_table_is_verified_record_by_record() -> None:
    table = frame_from_values(
        tuple(TypedColumn(name=field, type="str") for field in ODM_SCHEMA_FIELDS),
        [RECORD.split(","), [*RECORD.split(",")[:9], None, "34"]],
    )

    with pytest.raises(SourceError) as raised:
        odm_schema_sources(
            {"ODM": DatasetSource(path="odm.csv")}, {"ODM": table}, {"ODM"}
        )

    [diagnostic] = _diagnostics(raised.value)
    assert diagnostic["context"] == {
        "dataset": "ODM",
        "field": "ItemOID",
        "record": 2,
        "records": 1,
    }


def test_a_supplied_table_with_a_typed_schema_field_fails() -> None:
    columns = tuple(
        TypedColumn(name=field, type="int" if field == "Value" else "str")
        for field in ODM_SCHEMA_FIELDS
    )
    table = frame_from_values(columns, [[*RECORD.split(",")[:10], 34]])

    with pytest.raises(SourceError) as raised:
        odm_schema_sources(
            {"ODM": DatasetSource(path="odm.csv")}, {"ODM": table}, {"ODM"}
        )

    [diagnostic] = _diagnostics(raised.value)
    assert diagnostic["condition"] == "odm_schema_field_type"
    assert diagnostic["context"]["field"] == "Value"
