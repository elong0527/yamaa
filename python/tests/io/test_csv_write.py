"""R020 csv writer: exact bytes, quoting, missing, and display precision."""

from __future__ import annotations

from pathlib import Path

import pytest

from yamaa.io import write_artifact
from yamaa.io.csv_write import render_csv
from yamaa.io.polars import frame_from_values
from yamaa.io.publish import ArtifactError
from yamaa.models import DateTimeValue, DateValue, TypedColumn, TypedTable
from yamaa.specification.models import Output

REPOSITORY = Path(__file__).parents[3]

DM_COLUMNS = (
    TypedColumn(name="STUDYID", type="str"),
    TypedColumn(name="DOMAIN", type="str"),
    TypedColumn(name="USUBJID", type="str"),
    TypedColumn(name="SUBJID", type="str"),
    TypedColumn(name="SEX", type="str"),
    TypedColumn(name="AGE", type="int"),
    TypedColumn(name="ARM", type="str"),
    TypedColumn(name="ACTARM", type="str"),
)

DM_ROWS = [
    ["STUDY01", "DM", "001", "001", "M", 34, "Placebo", "Placebo"],
    ["STUDY01", "DM", "002", "002", "F", 28, "Vitamin D3", "Vitamin D3"],
    ["STUDY01", "DM", "003", "003", "U", None, "Unassigned", "Unassigned"],
    ["STUDY01", "DM", "004", "004", "U", None, "Unassigned", "Unassigned"],
]


def _table(
    columns: tuple[TypedColumn, ...] = DM_COLUMNS, rows: list | None = None
) -> TypedTable:
    return frame_from_values(columns, DM_ROWS if rows is None else rows)


def test_dm_artifact_matches_committed_bytes() -> None:
    expected = (
        REPOSITORY / "yaml" / "examples" / "sdtm-dm-basic" / "expected" / "dm.csv"
    ).read_bytes()

    assert (
        render_csv(
            _table(),
            [column.name for column in DM_COLUMNS],
            ["STUDYID", "USUBJID"],
            None,
        )
        == expected
    )


def test_quoting_edges_match_the_exact_condition() -> None:
    columns = (TypedColumn(name="TEXT", type="str"),)
    table = frame_from_values(
        columns,
        [
            ["plain"],
            ["has, comma"],
            ['say "hi"'],
            ["two\nlines"],
            ["ends\rhere"],
            [""],
            [None],
        ],
    )

    assert render_csv(table, ["TEXT"], [], None) == (
        b'TEXT\nplain\n"has, comma"\n"say ""hi"""\n"two\nlines"\n"ends\rhere"\n""\n\n'
    )


def test_header_only_artifact_keeps_header_and_terminator() -> None:
    assert render_csv(_table(rows=[]), ["STUDYID", "AGE"], [], None) == (
        b"STUDYID,AGE\n"
    )


def test_value_text_follows_column_type() -> None:
    columns = (
        TypedColumn(name="FLOAT", type="float"),
        TypedColumn(name="DATE", type="date"),
        TypedColumn(name="DATETIME", type="datetime"),
    )
    table = frame_from_values(
        columns,
        [
            [
                2.675,
                DateValue(year=2024, month=2, day=29),
                DateTimeValue(year=2024, month=1, day=2, hour=3, minute=4, second=5),
            ]
        ],
    )

    assert render_csv(table, ["FLOAT", "DATE", "DATETIME"], [], None) == (
        b"FLOAT,DATE,DATETIME\n2.675,2024-02-29,2024-01-02T03:04:05\n"
    )


@pytest.mark.parametrize(
    ("value", "decimals", "text"),
    [
        (0.125, 2, "0.13"),
        (-0.125, 2, "-0.13"),
        (2.675, 2, "2.67"),
        (25.0, 4, "25.0000"),
        (-0.001, 2, "0.00"),
        (2.5, 0, "3"),
    ],
)
def test_display_rounding_is_exact_half_away(
    value: float, decimals: int, text: str
) -> None:
    columns = (TypedColumn(name="DOSE", type="float"),)
    table = frame_from_values(columns, [[value]])

    assert render_csv(table, ["DOSE"], [], decimals) == (f"DOSE\n{text}\n".encode())


def test_display_rounding_supports_unbounded_declared_precision() -> None:
    columns = (TypedColumn(name="DOSE", type="float"),)
    table = frame_from_values(columns, [[25.0]])

    assert render_csv(table, ["DOSE"], [], 5000) == (
        b"DOSE\n25." + (b"0" * 5000) + b"\n"
    )


def test_unknown_output_column_fails() -> None:
    with pytest.raises(ArtifactError) as raised:
        render_csv(_table(), ["STUDYID", "ABSENT"], [], None)

    assert raised.value.condition == "unknown_output_column"


def test_profile_dispatch_rejects_unknown_extension_and_stray_decimals() -> None:
    table = _table()
    with pytest.raises(ArtifactError) as raised:
        write_artifact(table, Output(path="dm.txt", columns=["STUDYID"]), [])
    assert raised.value.condition == "unknown_artifact_profile"

    with pytest.raises(ArtifactError) as raised:
        write_artifact(
            table, Output(path="dm.parquet", columns=["STUDYID"], decimals=2), []
        )
    assert raised.value.condition == "decimals_not_applicable"

    with pytest.raises(ArtifactError) as raised:
        render_csv(table, ["STUDYID"], [], -1)
    assert raised.value.condition == "invalid_output_decimals"
