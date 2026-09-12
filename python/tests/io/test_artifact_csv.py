from __future__ import annotations

from pathlib import Path

import pytest

from yamaa.io import ArtifactError, build_artifact, render_csv
from yamaa.io.csv import fixed_point
from yamaa.io.polars import frame_from_values
from yamaa.models import DateTimeValue, DateValue, TypedColumn, TypedTable
from yamaa.specification.models import ColumnType, Output

EXAMPLES = Path(__file__).parents[3] / "yaml" / "examples"


def table(
    declared: list[tuple[str, ColumnType]], rows: list[list[object]]
) -> TypedTable:
    columns = tuple(TypedColumn(name=name, type=kind) for name, kind in declared)
    return frame_from_values(columns, rows)


def rendered(
    declared: list[tuple[str, ColumnType]],
    rows: list[list[object]],
    *,
    columns: list[str] | None = None,
    decimals: int | None = None,
    keys: list[str] | None = None,
) -> bytes:
    completed = table(declared, rows)
    names = columns or [name for name, _ in declared]
    output = Output(path="out.csv", decimals=decimals, columns=names)
    return render_csv(build_artifact(completed, output, keys or names[:1]))


DM_COLUMNS: list[tuple[str, ColumnType]] = [
    ("STUDYID", "str"),
    ("DOMAIN", "str"),
    ("USUBJID", "str"),
    ("SUBJID", "str"),
    ("SEX", "str"),
    ("AGE", "int"),
    ("ARM", "str"),
    ("ACTARM", "str"),
]


def test_a_constructed_dm_table_serializes_to_the_committed_expected_bytes() -> None:
    rows: list[list[object]] = [
        ["STUDY01", "DM", "001", "001", "M", 34, "Placebo", "Placebo"],
        ["STUDY01", "DM", "002", "002", "F", 28, "Vitamin D3", "Vitamin D3"],
        ["STUDY01", "DM", "003", "003", "U", None, "Unassigned", "Unassigned"],
        ["STUDY01", "DM", "004", "004", "U", None, "Unassigned", "Unassigned"],
    ]

    content = rendered(DM_COLUMNS, rows, keys=["STUDYID", "USUBJID"])

    assert content == (EXAMPLES / "sdtm-dm-basic" / "expected" / "dm.csv").read_bytes()


def test_reported_precision_rounds_once_at_the_written_field() -> None:
    declared: list[tuple[str, ColumnType]] = [
        ("STUDYID", "str"),
        ("USUBJID", "str"),
        ("PARAMCD", "str"),
        ("AVAL", "float"),
        ("ANRLO", "float"),
        ("R2ANRLO", "float"),
    ]
    rows: list[list[object]] = [
        ["CTX", "CTX-01", "ALT", 44.0, 32.0, 44.0 / 32.0],
        ["CTX", "CTX-02", "ALT", 73.5, 32.0, 73.5 / 32.0],
        ["CTX", "CTX-03", "ALT", 110.0, 32.0, 110.0 / 32.0],
        ["CTX", "CTX-04", "ALT", 1.0, 32.0, 1.0 / 32.0],
        ["CTX", "CTX-05", "ALT", 58.0, None, None],
    ]

    content = rendered(declared, rows, decimals=4, keys=["STUDYID", "USUBJID"])

    committed = (
        EXAMPLES / "adam-adlb-reported-precision" / "expected" / "adlb.csv"
    ).read_bytes()
    assert content == b"\n".join(committed.split(b"\n")[:6]) + b"\n"


@pytest.mark.parametrize(
    ("value", "decimals", "text"),
    [
        (0.125, 2, "0.13"),
        (-0.125, 2, "-0.13"),
        (2.675, 2, "2.67"),
        (25.0, 4, "25.0000"),
        (1.0 / 32.0, 4, "0.0313"),
        (0.5, 0, "1"),
        (1.5, 0, "2"),
        (-0.5, 0, "-1"),
        (-0.004, 2, "0.00"),
        (0.0, 3, "0.000"),
        (-0.0, 3, "0.000"),
    ],
)
def test_display_rounding_is_exact_and_breaks_ties_away_from_zero(
    value: float, decimals: int, text: str
) -> None:
    assert fixed_point(value, decimals) == text


def test_a_large_declared_precision_stays_exact_rather_than_reaching_a_host_limit() -> (
    None
):
    text = fixed_point(1e308, 4000)

    # The host's own exact conversion agrees wherever no tie is broken,
    # which is what makes it a check of the digits rather than a copy.
    assert text == f"{1e308:.4000f}"
    assert len(text) == 309 + 1 + 4000
    assert fixed_point(0.125, 5000) == "0.125" + "0" * 4997


def test_float_without_a_declared_precision_keeps_r011_text() -> None:
    content = rendered(
        [("ID", "str"), ("VALUE", "float")],
        [["1", 1.0], ["2", 0.1], ["3", 1e22], ["4", -0.0]],
    )

    assert content == b"ID,VALUE\n1,1\n2,0.1\n3,10000000000000000000000\n4,-0\n"


def test_quoting_covers_every_field_the_condition_names_and_no_other() -> None:
    content = rendered(
        [("ID", "str"), ("TEXT", "str")],
        [
            ["1", "plain text"],
            ["2", "has, comma"],
            ["3", 'say "hi"'],
            ["4", "two\nlines"],
            ["5", "carriage\rreturn"],
            ["6", ""],
            ["7", None],
            ["8", " kept "],
        ],
    )

    assert content == (
        b"ID,TEXT\n"
        b"1,plain text\n"
        b'2,"has, comma"\n'
        b'3,"say ""hi"""\n'
        b'4,"two\nlines"\n'
        b'5,"carriage\rreturn"\n'
        b'6,""\n'
        b"7,\n"
        b"8, kept \n"
    )


def test_an_artifact_with_no_rows_is_its_header_record_alone() -> None:
    assert rendered([("STUDYID", "str"), ("AGE", "int")], []) == b"STUDYID,AGE\n"


def test_declared_types_take_the_text_their_owning_rule_fixes() -> None:
    content = rendered(
        [
            ("ID", "str"),
            ("COUNT", "int"),
            ("WHEN", "date"),
            ("AT", "datetime"),
            ("TEXT", "str"),
        ],
        [
            [
                "1",
                -42,
                DateValue.parse("2020-01-02"),
                DateTimeValue.parse("2020-01-02T03:04:05"),
                "\u00e9\U0001d400",
            ],
            ["2", 0, None, None, None],
        ],
    )

    assert content == (
        "ID,COUNT,WHEN,AT,TEXT\n"
        "1,-42,2020-01-02,2020-01-02T03:04:05,\u00e9\U0001d400\n"
        "2,0,,,\n"
    ).encode("utf-8")
    assert not content.startswith(b"\xef\xbb\xbf")


def test_a_temporal_value_at_the_edge_of_the_calendar_still_writes() -> None:
    content = rendered(
        [("ID", "str"), ("WHEN", "date"), ("AT", "datetime")],
        [
            [
                "1",
                DateValue.parse("0001-01-01"),
                DateTimeValue.parse("0001-01-01T00:00:00"),
            ],
            [
                "2",
                DateValue.parse("9999-12-31"),
                DateTimeValue.parse("9999-12-31T23:59:59"),
            ],
        ],
    )

    assert content == (
        b"ID,WHEN,AT\n"
        b"1,0001-01-01,0001-01-01T00:00:00\n"
        b"2,9999-12-31,9999-12-31T23:59:59\n"
    )


def test_output_columns_select_and_order_the_artifact() -> None:
    content = rendered(
        [("A", "str"), ("INTERNAL", "int"), ("B", "str")],
        [["a", 1, "b"]],
        columns=["B", "A"],
        keys=["B"],
    )

    assert content == b"B,A\nb,a\n"


def test_order_by_applies_each_term_with_its_own_direction_and_nulls() -> None:
    completed = table(
        [("SITE", "str"), ("SEQ", "int"), ("ROW", "int")],
        [
            ["b", 1, 0],
            [None, 2, 1],
            ["a", None, 2],
            ["a", 5, 3],
            ["a", 5, 4],
        ],
    )
    output = Output(
        path="out.csv",
        columns=["SITE", "SEQ", "ROW"],
        order_by=[
            {"variable": "SITE", "direction": "asc", "nulls": "first"},
            {"variable": "SEQ", "direction": "desc", "nulls": "last"},
        ],
    )

    content = render_csv(build_artifact(completed, output, ["ROW"]))

    assert content == b"SITE,SEQ,ROW\n,2,1\na,5,3\na,5,4\na,,2\nb,1,0\n"


def test_an_unwritable_stored_value_reports_its_column_row_key_and_value() -> None:
    completed = table(
        [("USUBJID", "str"), ("VALUE", "float")],
        [["S-1", 1.0], ["S-2", float("nan")], ["S-3", float("inf")]],
    )
    output = Output(path="out.csv", columns=["USUBJID", "VALUE"])

    with pytest.raises(ArtifactError) as raised:
        build_artifact(completed, output, ["USUBJID"])

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.phase == "output"
    assert diagnostic.condition == "unwritable_value"
    assert diagnostic.requirement == "R020-46"
    assert diagnostic.context["column"] == "VALUE"
    assert diagnostic.context["failure_count"] == 2
    assert diagnostic.context["keys"] == [{"USUBJID": "S-2"}, {"USUBJID": "S-3"}]
    assert diagnostic.context["value"] == "nan"
