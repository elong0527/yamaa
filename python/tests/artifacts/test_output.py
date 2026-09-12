from __future__ import annotations

import datetime as dt

import polars as pl
import pytest

from yamaa.artifacts import (
    ArtifactError,
    artifact_profile,
    build_artifact,
    profile_of,
)
from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn, TypedTable
from yamaa.specification.models import ColumnType, Output

DECLARED: list[tuple[str, ColumnType]] = [
    ("STUDYID", "str"),
    ("USUBJID", "str"),
    ("RANK", "int"),
]


def table(rows: list[list[object]] | None = None) -> TypedTable:
    columns = tuple(TypedColumn(name=name, type=kind) for name, kind in DECLARED)
    return frame_from_values(columns, rows if rows is not None else [["S", "S-1", 1]])


def diagnostics(output: Output, keys: list[str]) -> list[tuple[str, str, str]]:
    with pytest.raises(ArtifactError) as raised:
        build_artifact(table(), output, keys)
    return [
        (item.condition, item.spec_paths[0], str(item.context.get("column", "")))
        for item in raised.value.diagnostics
    ]


@pytest.mark.parametrize(
    ("path", "profile"),
    [
        ("adsl.csv", "csv"),
        ("ADSL.CSV", "csv"),
        ("out/adsl.Parquet", "parquet"),
    ],
)
def test_the_extension_selects_the_profile_without_regard_to_case(
    path: str, profile: str
) -> None:
    assert artifact_profile(path) == profile
    assert profile_of(path) == profile


@pytest.mark.parametrize("path", ["adsl", "adsl.xpt", "adsl.csv.gz", "adsl."])
def test_an_extension_outside_the_mapping_names_no_profile(path: str) -> None:
    assert profile_of(path) is None
    with pytest.raises(ArtifactError) as raised:
        artifact_profile(path)

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "unknown_artifact_profile"
    assert diagnostic.requirement == "R020-43"
    assert diagnostic.context["path"] == path


def test_output_columns_must_cover_declared_columns_exactly_once() -> None:
    assert diagnostics(
        Output(path="adsl.csv", columns=["STUDYID", "STUDYID", "ABSENT"]),
        ["STUDYID"],
    ) == [
        ("duplicate_identifier", "output.columns[1]", "STUDYID"),
        ("undeclared_column", "output.columns[2]", "ABSENT"),
    ]


def test_a_key_must_be_an_output_column_rather_than_an_internal_one() -> None:
    assert diagnostics(
        Output(path="adsl.csv", columns=["STUDYID", "USUBJID"]),
        ["STUDYID", "RANK"],
    ) == [("internal_column_in_keys", "keys[1]", "RANK")]


def test_an_order_term_names_a_declared_column_once() -> None:
    assert diagnostics(
        Output(
            path="adsl.csv",
            columns=["STUDYID", "USUBJID"],
            order_by=[
                {"variable": "RANK"},
                {"variable": "RANK"},
                {"variable": "ABSENT"},
            ],
        ),
        ["STUDYID"],
    ) == [
        ("duplicate_order_term", "output.order_by[1]", "RANK"),
        ("undeclared_column", "output.order_by[2]", "ABSENT"),
    ]


def test_an_internal_column_may_order_the_artifact_it_does_not_appear_in() -> None:
    completed = table([["S", "S-2", 1], ["S", "S-1", 2]])
    output = Output(
        path="adsl.csv",
        columns=["STUDYID", "USUBJID"],
        order_by=[{"variable": "RANK", "direction": "desc"}],
    )

    built = build_artifact(completed, output, ["STUDYID", "USUBJID"])

    assert built.frame.columns == ["STUDYID", "USUBJID"]
    assert built.frame.get_column("USUBJID").to_list() == ["S-1", "S-2"]


def test_a_negative_display_precision_is_refused() -> None:
    with pytest.raises(ArtifactError) as raised:
        build_artifact(
            table(),
            Output(path="adsl.csv", decimals=-1, columns=["STUDYID"]),
            ["STUDYID"],
        )

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "invalid_field_type"
    assert diagnostic.requirement == "R020-44"


def test_a_column_stored_outside_its_declared_host_type_is_unwritable() -> None:
    columns = (
        TypedColumn(name="USUBJID", type="str"),
        TypedColumn(name="AT", type="datetime"),
    )
    zoned = TypedTable(
        columns=columns,
        frame=pl.DataFrame(
            [
                pl.Series("USUBJID", ["S-1"], dtype=pl.String),
                pl.Series(
                    "AT",
                    [dt.datetime(2020, 1, 2, 3, 4, 5, tzinfo=dt.UTC)],
                    dtype=pl.Datetime("us", "UTC"),
                ),
            ]
        ),
    )
    output = Output(path="adsl.csv", columns=["USUBJID", "AT"])

    with pytest.raises(ArtifactError) as raised:
        build_artifact(zoned, output, ["USUBJID"])

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "unwritable_value"
    assert diagnostic.context["column"] == "AT"
    assert diagnostic.context["stored"] == "Datetime(time_unit='us', time_zone='UTC')"


def test_a_datetime_below_whole_seconds_is_unwritable() -> None:
    columns = (
        TypedColumn(name="USUBJID", type="str"),
        TypedColumn(name="AT", type="datetime"),
    )
    finer = TypedTable(
        columns=columns,
        frame=pl.DataFrame(
            [
                pl.Series("USUBJID", ["S-1"], dtype=pl.String),
                pl.Series(
                    "AT",
                    # R016 datetimes are zone-free; the finer resolution
                    # is the defect under test.
                    [dt.datetime(2020, 1, 2, 3, 4, 5, 500000)],  # noqa: DTZ001
                    dtype=pl.Datetime("us"),
                ),
            ]
        ),
    )
    output = Output(path="adsl.parquet", columns=["USUBJID", "AT"])

    with pytest.raises(ArtifactError) as raised:
        build_artifact(finer, output, ["USUBJID"])

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "unwritable_value"
    assert diagnostic.requirement == "R020-46"
    assert diagnostic.context["keys"] == [{"USUBJID": "S-1"}]
    # R020-23 stores a datetime as microseconds from the epoch, and the
    # stored count is what the diagnostic reports.
    assert diagnostic.context["value"] == 1577934245500000


def test_a_date_outside_the_calendar_is_unwritable() -> None:
    columns = (
        TypedColumn(name="USUBJID", type="str"),
        TypedColumn(name="WHEN", type="date"),
    )
    beyond = TypedTable(
        columns=columns,
        frame=pl.DataFrame(
            [
                pl.Series("USUBJID", ["S-1"], dtype=pl.String),
                pl.Series("WHEN", [10_000_000], dtype=pl.Int32).cast(pl.Date),
            ]
        ),
    )
    output = Output(path="adsl.csv", columns=["USUBJID", "WHEN"])

    with pytest.raises(ArtifactError) as raised:
        build_artifact(beyond, output, ["USUBJID"])

    diagnostic = raised.value.diagnostics[0]
    assert diagnostic.condition == "unwritable_value"
    assert diagnostic.context["column"] == "WHEN"
    assert diagnostic.context["value"] == 10_000_000
