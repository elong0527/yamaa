from __future__ import annotations

import datetime as dt
from pathlib import Path

import polars as pl
import pyarrow as pa
import pyarrow.parquet as pq
import pytest

from yamaa.io import (
    ProducerSchemaUnresolved,
    SourceError,
    load_source_table,
    load_source_tables,
)
from yamaa.io.polars import frame_from_values
from yamaa.io.project import ProjectResources
from yamaa.models import DateValue, TypedColumn
from yamaa.specification.models import DatasetSource

REPOSITORY = Path(__file__).parents[3]


def _diagnostic(error: SourceError) -> dict[str, object]:
    assert len(error.diagnostics) == 1
    return error.diagnostics[0].model_dump(mode="python")


def test_loads_ordered_typed_polars_table_without_inference(tmp_path: Path) -> None:
    (tmp_path / "dm.csv").write_bytes(
        b"ID,AGE,SCORE,DATE,MOMENT,EMPTY,TEXT\n"
        b'007,42,1.5,2025-01-02,2025-01-02T03:04,,""\n'
        b"008,,.Inf,2025-02-03,2025-02-03T04:05:06,NA,unknown\n"
    )
    source = DatasetSource(
        path="dm.csv",
        types={
            "AGE": "int",
            "SCORE": "float",
            "DATE": "date",
            "MOMENT": "datetime",
        },
    )

    loaded = load_source_table("DM", source, ProjectResources(tmp_path))

    assert loaded.written_path == "dm.csv"
    assert loaded.table.frame.columns == [
        "ID",
        "AGE",
        "SCORE",
        "DATE",
        "MOMENT",
        "EMPTY",
        "TEXT",
    ]
    assert loaded.table.frame.schema == {
        "ID": pl.String,
        "AGE": pl.Int64,
        "SCORE": pl.Float64,
        "DATE": pl.Date,
        "MOMENT": pl.Datetime("us"),
        "EMPTY": pl.String,
        "TEXT": pl.String,
    }
    rows = loaded.table.frame.to_dicts()
    assert rows[0] == {
        "ID": "007",
        "AGE": 42,
        "SCORE": 1.5,
        "DATE": dt.date(2025, 1, 2),
        # R016 datetimes are zone-free local civil times.
        "MOMENT": dt.datetime(2025, 1, 2, 3, 4),  # noqa: DTZ001
        "EMPTY": None,
        "TEXT": None,
    }
    assert rows[1]["ID"] == "008"
    assert rows[1]["AGE"] is None
    assert rows[1]["SCORE"] is None
    assert rows[1]["EMPTY"] == "NA"
    assert rows[1]["TEXT"] == "unknown"
    # Native temporal columns answer ordinary Polars expressions; an object
    # column of R016 values would not.
    assert loaded.table.frame.filter(pl.col("DATE") > dt.date(2025, 1, 15)).height == 1


def test_a_date_below_day_precision_stores_the_day_it_names() -> None:
    # Ingestion never produces one, because R011 admits only the complete
    # R016 forms, but `date_impute` does. REQ-0570 makes collected precision
    # unobservable outside the derivation, so the column carries the fields
    # and a specification needing the precision derives it from
    # `date_precision` instead.
    column = TypedColumn(name="DATE", type="date")
    partial = DateValue(year=2025, month=2, day=1, collected_precision="month")

    table = frame_from_values((column,), [[partial]])

    assert table.frame.item() == dt.date(2025, 2, 1)


@pytest.mark.parametrize("text", ["NA", "NULL", ".", "unknown"])
def test_missing_looking_text_remains_an_ordinary_string(
    tmp_path: Path, text: str
) -> None:
    (tmp_path / "dm.csv").write_text(f"VALUE\n{text}\n")
    loaded = load_source_table(
        "DM", DatasetSource(path="dm.csv"), ProjectResources(tmp_path)
    )
    assert loaded.table.frame.item() == text


def test_csv_profile_extension_is_case_insensitive(tmp_path: Path) -> None:
    (tmp_path / "DM.CSV").write_text("ID\n001\n")

    loaded = load_source_table(
        "DM", DatasetSource(path="DM.CSV"), ProjectResources(tmp_path)
    )

    assert loaded.table.frame.item() == "001"


def test_loads_parquet_embedded_types_without_inference(tmp_path: Path) -> None:
    path = tmp_path / "dm.parquet"
    table = pa.Table.from_arrays(
        [
            pa.array(["007", "008"], type=pa.string()),
            pa.array([42, None], type=pa.int64()),
            pa.array([1.5, float("inf")], type=pa.float64()),
            pa.array([dt.date(2025, 1, 2), dt.date(2025, 2, 3)], type=pa.date32()),
            pa.array(
                [dt.datetime(2025, 1, 2, 3, 4), None],  # noqa: DTZ001
                type=pa.timestamp("us"),
            ),
            pa.array(["", None], type=pa.string()),
        ],
        names=["ID", "AGE", "SCORE", "DATE", "MOMENT", "TEXT"],
    )
    pq.write_table(table, path)

    loaded = load_source_table(
        "DM", DatasetSource(path="dm.parquet"), ProjectResources(tmp_path)
    )

    assert loaded.table.columns == (
        TypedColumn(name="ID", type="str"),
        TypedColumn(name="AGE", type="int"),
        TypedColumn(name="SCORE", type="float"),
        TypedColumn(name="DATE", type="date"),
        TypedColumn(name="MOMENT", type="datetime"),
        TypedColumn(name="TEXT", type="str"),
    )
    assert loaded.table.frame.to_dicts() == [
        {
            "ID": "007",
            "AGE": 42,
            "SCORE": 1.5,
            "DATE": dt.date(2025, 1, 2),
            "MOMENT": dt.datetime(2025, 1, 2, 3, 4),  # noqa: DTZ001
            # REQ-1159: the default empty-string convention reads a stored
            # zero-length string in a str field as the missing value.
            "TEXT": None,
        },
        {
            "ID": "008",
            "AGE": None,
            "SCORE": None,
            "DATE": dt.date(2025, 2, 3),
            "MOMENT": None,
            "TEXT": None,
        },
    ]


def _write_parquet_with_empty_string(tmp_path: Path) -> None:
    pq.write_table(
        pa.table(
            {
                "ECENDTC": pa.array(["2025-01-01", "", None], type=pa.string()),
                "ECDOSE": pa.array([50.0, 25.0, 10.0], type=pa.float64()),
            }
        ),
        tmp_path / "ec.parquet",
    )


def test_parquet_empty_string_defaults_to_missing(tmp_path: Path) -> None:
    _write_parquet_with_empty_string(tmp_path)

    loaded = load_source_table(
        "EC", DatasetSource(path="ec.parquet"), ProjectResources(tmp_path)
    )

    assert loaded.table.frame.to_dicts() == [
        {"ECENDTC": "2025-01-01", "ECDOSE": 50.0},
        # REQ-1159: the stored empty string reads as missing; the float
        # column is untouched by the str-only convention (REQ-1160).
        {"ECENDTC": None, "ECDOSE": 25.0},
        {"ECENDTC": None, "ECDOSE": 10.0},
    ]


def test_parquet_empty_string_present_keeps_collected_empty(
    tmp_path: Path,
) -> None:
    _write_parquet_with_empty_string(tmp_path)

    loaded = load_source_table(
        "EC",
        DatasetSource(path="ec.parquet", empty_string="present"),
        ProjectResources(tmp_path),
    )

    assert loaded.table.frame.to_dicts() == [
        {"ECENDTC": "2025-01-01", "ECDOSE": 50.0},
        {"ECENDTC": "", "ECDOSE": 25.0},
        {"ECENDTC": None, "ECDOSE": 10.0},
    ]


def test_csv_rejects_present_empty_string_convention(tmp_path: Path) -> None:
    (tmp_path / "ec.csv").write_bytes(b"ECENDTC\n2025-01-01\n")

    with pytest.raises(SourceError) as error:
        load_source_table(
            "EC",
            DatasetSource(path="ec.csv", empty_string="present"),
            ProjectResources(tmp_path),
        )

    diagnostic = _diagnostic(error.value)
    assert diagnostic["phase"] == "validation"
    assert diagnostic["condition"] == "empty_string_present_unsupported"
    assert diagnostic["requirement"] == "REQ-1161"
    assert diagnostic["spec_paths"] == ("input.EC.empty_string",)


def test_parquet_profile_extension_is_case_insensitive(tmp_path: Path) -> None:
    path = tmp_path / "DM.PARQUET"
    pq.write_table(pa.table({"ID": pa.array(["001"], type=pa.string())}), path)

    loaded = load_source_table(
        "DM", DatasetSource(path="DM.PARQUET"), ProjectResources(tmp_path)
    )

    assert loaded.table.frame.item() == "001"


def test_parquet_rejects_inline_types_before_snapshot_bytes_are_read(
    tmp_path: Path,
) -> None:
    path = tmp_path / "dm.parquet"
    pq.write_table(pa.table({"ID": pa.array(["001"], type=pa.string())}), path)
    resources = ProjectResources(tmp_path)

    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM",
            DatasetSource(path="dm.parquet", types={"ID": "str"}),
            resources,
        )

    assert _diagnostic(raised.value) == {
        "phase": "validation",
        "condition": "redundant_field_type",
        "spec_paths": ("input.DM.types.ID",),
        "requirement": "REQ-0533",
        "context": {"dataset": "DM", "field": "ID", "type": "str"},
    }
    assert resources.capture_reads == 0


def test_parquet_rejects_an_unsupported_embedded_type(tmp_path: Path) -> None:
    path = tmp_path / "dm.parquet"
    pq.write_table(pa.table({"FLAG": pa.array([True], type=pa.bool_())}), path)

    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM", DatasetSource(path="dm.parquet"), ProjectResources(tmp_path)
        )

    assert _diagnostic(raised.value) == {
        "phase": "ingest",
        "condition": "source_field_type_unsupported",
        "spec_paths": ("input.DM.path",),
        "requirement": "REQ-1040",
        "context": {
            "dataset": "DM",
            "path": "dm.parquet",
            "field": "FLAG",
            "stored_type": "bool",
        },
    }


@pytest.mark.parametrize(
    ("names", "condition", "field"),
    [
        ([""], "source_field_name_empty", 1),
        (["ID", "ID"], "source_field_name_duplicate", "ID"),
    ],
)
def test_parquet_rejects_invalid_field_names(
    tmp_path: Path,
    names: list[str],
    condition: str,
    field: int | str,
) -> None:
    path = tmp_path / "dm.parquet"
    pq.write_table(
        pa.Table.from_arrays(
            [pa.array(["x"], type=pa.string()) for _ in names], names=names
        ),
        path,
    )

    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM", DatasetSource(path="dm.parquet"), ProjectResources(tmp_path)
        )

    assert _diagnostic(raised.value) == {
        "phase": "ingest",
        "condition": condition,
        "spec_paths": ("input.DM.path",),
        "requirement": "REQ-1039",
        "context": {
            "dataset": "DM",
            "path": "dm.parquet",
            "field": field,
        },
    }


def test_parquet_rejects_a_datetime_below_whole_seconds(tmp_path: Path) -> None:
    path = tmp_path / "dm.parquet"
    pq.write_table(
        pa.table({"AT": pa.array([1], type=pa.timestamp("us"))}),
        path,
    )

    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM", DatasetSource(path="dm.parquet"), ProjectResources(tmp_path)
        )

    assert _diagnostic(raised.value) == {
        "phase": "ingest",
        "condition": "source_field_value_invalid",
        "spec_paths": ("input.DM.path",),
        "requirement": "REQ-1041",
        "context": {
            "dataset": "DM",
            "path": "dm.parquet",
            "field": "AT",
            "row": 1,
            "value": 1,
        },
    }


def test_parquet_rejects_invalid_container_bytes(tmp_path: Path) -> None:
    (tmp_path / "dm.parquet").write_bytes(b"not parquet")

    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM", DatasetSource(path="dm.parquet"), ProjectResources(tmp_path)
        )

    assert _diagnostic(raised.value) == {
        "phase": "ingest",
        "condition": "source_parquet_invalid",
        "spec_paths": ("input.DM.path",),
        "requirement": "REQ-1038",
        "context": {"dataset": "DM", "path": "dm.parquet"},
    }


def test_parquet_rejects_an_empty_schema(tmp_path: Path) -> None:
    path = tmp_path / "dm.parquet"
    pq.write_table(pa.table({}), path)

    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM", DatasetSource(path="dm.parquet"), ProjectResources(tmp_path)
        )

    assert _diagnostic(raised.value) == {
        "phase": "ingest",
        "condition": "source_parquet_invalid",
        "spec_paths": ("input.DM.path",),
        "requirement": "REQ-1038",
        "context": {"dataset": "DM", "path": "dm.parquet"},
    }


def test_adae_fixture_treats_bare_and_quoted_empty_as_missing() -> None:
    root = REPOSITORY / "benchmarks/adam-adae-text-cleanup"

    loaded = load_source_table(
        "AE",
        DatasetSource(path="input/ae.csv"),
        ProjectResources(root),
    )

    assert loaded.table.frame["AESPID"].to_list() == [
        "AE-001",
        None,
        "BAD-ID",
        "AE-104",
        None,
    ]
    assert loaded.table.frame["AESEQ"].to_list() == ["1", "2", "3", "1", "2"]


def test_multiple_declarations_share_snapshot_but_keep_types(tmp_path: Path) -> None:
    (tmp_path / "values.csv").write_text("VALUE\n001\n")
    resources = ProjectResources(tmp_path)

    loaded = load_source_tables(
        {
            "TEXT": DatasetSource(path="values.csv"),
            "NUMBER": DatasetSource(path="values.csv", types={"VALUE": "int"}),
        },
        resources,
    )

    assert loaded["TEXT"].snapshot is loaded["NUMBER"].snapshot
    assert resources.capture_reads == 1
    assert loaded["TEXT"].table.frame.item() == "001"
    assert loaded["NUMBER"].table.frame.item() == 1


def test_header_only_source_retains_declared_schema(tmp_path: Path) -> None:
    (tmp_path / "empty.csv").write_text("ID,AGE\n")

    loaded = load_source_table(
        "DM",
        DatasetSource(path="empty.csv", types={"AGE": "int"}),
        ProjectResources(tmp_path),
    )

    assert loaded.table.frame.schema == {"ID": pl.String, "AGE": pl.Int64}
    assert loaded.table.frame.height == 0


def test_quoted_empty_is_missing_for_a_non_string_type(tmp_path: Path) -> None:
    (tmp_path / "dm.csv").write_bytes(b'AGE\n""\n')

    loaded = load_source_table(
        "DM",
        DatasetSource(path="dm.csv", types={"AGE": "int"}),
        ProjectResources(tmp_path),
    )

    assert loaded.table.frame["AGE"].to_list() == [None]


@pytest.mark.parametrize(
    ("example", "dataset", "path", "condition", "requirement"),
    [
        (
            "negative-path-absolute",
            "LBREF",
            "/shared/reference/lbref.csv",
            "resource_path_not_relative",
            "REQ-0781",
        ),
        (
            "negative-path-directory",
            "LBREF",
            "input/lbref",
            "resource_path_not_regular_file",
            "REQ-0785",
        ),
        (
            "negative-path-missing",
            "LBREF",
            "input/lbref.csv",
            "resource_path_missing",
            "REQ-0785",
        ),
        (
            "negative-path-parent-escape",
            "LBREF",
            "../reference/lbref.csv",
            "resource_path_outside_project",
            "REQ-0784",
        ),
        (
            "negative-path-symlink",
            "LBREF",
            "input/lbref.csv",
            "resource_path_symlink",
            "REQ-0783",
        ),
        (
            "negative-path-url",
            "LBREF",
            "https://reference.example.org/limits/lbref.csv",
            "resource_path_uri_scheme",
            "REQ-0775",
        ),
    ],
)
def test_path_fixtures_report_exact_diagnostics(
    example: str,
    dataset: str,
    path: str,
    condition: str,
    requirement: str,
) -> None:
    root = REPOSITORY / "benchmarks" / example
    with pytest.raises(SourceError) as raised:
        load_source_table(
            dataset,
            DatasetSource(path=path),
            ProjectResources(root),
        )

    assert _diagnostic(raised.value) == {
        "phase": "validation",
        "condition": condition,
        "spec_paths": (f"input.{dataset}.path",),
        "requirement": requirement,
        "context": {"dataset": dataset, "path": path},
    }


def test_committed_symlink_fixture_is_a_real_symlink() -> None:
    path = REPOSITORY / "benchmarks/negative-path-symlink/input/lbref.csv"
    assert path.is_symlink()


@pytest.mark.parametrize(
    ("example", "condition", "requirement", "context"),
    [
        (
            "negative-source-field-duplicate",
            "source_field_name_duplicate",
            "REQ-0851",
            {"record": 1, "field": "SEX"},
        ),
        (
            "negative-source-unnamed-field",
            "source_field_name_empty",
            "REQ-0851",
            {"record": 1, "field": 4},
        ),
        (
            "negative-source-bad-encoding",
            "invalid_text",
            "REQ-0029",
            {"record": 3, "field": 3},
        ),
        (
            "negative-source-extra-field",
            "source_record_width",
            "REQ-0851",
            {"record": 3, "field": 5},
        ),
        (
            "negative-source-unterminated-quote",
            "source_quote_unterminated",
            "REQ-0851",
            {"record": 3, "field": 3},
        ),
    ],
)
def test_csv_fixtures_report_exact_diagnostics(
    example: str,
    condition: str,
    requirement: str,
    context: dict[str, object],
) -> None:
    root = REPOSITORY / "benchmarks" / example
    path = "input/dm.csv"
    with pytest.raises(SourceError) as raised:
        load_source_table("DM", DatasetSource(path=path), ProjectResources(root))

    assert _diagnostic(raised.value) == {
        "phase": "ingest",
        "condition": condition,
        "spec_paths": ("input.DM.path",),
        "requirement": requirement,
        "context": {"dataset": "DM", "path": path, **context},
    }


def test_unknown_profile_fails_before_snapshot_bytes_are_read() -> None:
    root = REPOSITORY / "benchmarks/negative-source-unknown-format"
    resources = ProjectResources(root)

    with pytest.raises(SourceError) as raised:
        load_source_table("DM", DatasetSource(path="input/dm.txt"), resources)

    assert _diagnostic(raised.value) == {
        "phase": "validation",
        "condition": "source_profile_unknown",
        "spec_paths": ("input.DM.path",),
        "requirement": "REQ-0852",
        "context": {"dataset": "DM", "path": "input/dm.txt"},
    }
    assert resources.capture_reads == 0


def test_all_source_declarations_validate_before_any_snapshot_read(
    tmp_path: Path,
) -> None:
    (tmp_path / "dm.csv").write_text("ID\n001\n")
    (tmp_path / "terms.txt").write_text("TERM\nheadache\n")
    resources = ProjectResources(tmp_path)

    with pytest.raises(SourceError):
        load_source_tables(
            {
                "DM": DatasetSource(path="dm.csv"),
                "TERMS": DatasetSource(path="terms.txt"),
            },
            resources,
        )

    assert resources.capture_reads == 0


@pytest.mark.parametrize(
    ("example", "field", "target", "value"),
    [
        ("negative-source-na-age", "AGE", "int", "NA"),
        ("negative-ingest-unit", "EXDOSE", "float", "200 mg"),
    ],
)
def test_typed_parse_fixtures_are_ingestion_failures(
    example: str, field: str, target: str, value: str
) -> None:
    root = REPOSITORY / "benchmarks" / example
    path = "input/dm.csv" if field == "AGE" else "input/ex.csv"
    dataset = "DM" if field == "AGE" else "EX"
    with pytest.raises(SourceError) as raised:
        load_source_table(
            dataset,
            DatasetSource(path=path, types={field: target}),
            ProjectResources(root),
        )

    assert _diagnostic(raised.value) == {
        "phase": "ingest",
        "condition": "field_parse_failed",
        "spec_paths": (f"input.{dataset}.types.{field}",),
        "requirement": "REQ-0536",
        "context": {
            "dataset": dataset,
            "field": field,
            "type": target,
            "value": value,
        },
    }


def test_unknown_typed_field_fails_in_validation(tmp_path: Path) -> None:
    (tmp_path / "dm.csv").write_text("ID,AGE\n001,42\n")

    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM",
            DatasetSource(path="dm.csv", types={"AGEYRS": "int"}),
            ProjectResources(tmp_path),
        )

    assert _diagnostic(raised.value) == {
        "phase": "validation",
        "condition": "unknown_field",
        "spec_paths": ("input.DM.types.AGEYRS",),
        "requirement": "REQ-0532",
        "context": {"dataset": "DM", "field": "AGEYRS"},
    }


def test_producer_link_requires_workflow_resolution(tmp_path: Path) -> None:
    (tmp_path / "dm.csv").write_text("ID\n001\n")

    with pytest.raises(ProducerSchemaUnresolved) as raised:
        load_source_table(
            "DM",
            DatasetSource(path="dm.csv", schema="dm.schema.yaml"),
            ProjectResources(tmp_path),
        )

    assert raised.value.datasets == ("DM",)


def test_unresolved_producer_links_preserve_declaration_order(tmp_path: Path) -> None:
    (tmp_path / "lb.csv").write_text("ID\n001\n")
    (tmp_path / "dm.csv").write_text("ID\n001\n")

    with pytest.raises(ProducerSchemaUnresolved) as raised:
        load_source_tables(
            {
                "LB": DatasetSource(path="lb.csv", schema="lb.schema.yaml"),
                "DM": DatasetSource(path="dm.csv", schema="dm.schema.yaml"),
            },
            ProjectResources(tmp_path),
        )

    assert raised.value.datasets == ("LB", "DM")


def test_producer_link_path_failure_precedes_workflow_resolution(
    tmp_path: Path,
) -> None:
    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM",
            DatasetSource(path="missing.csv", schema="dm.schema.yaml"),
            ProjectResources(tmp_path),
        )

    assert _diagnostic(raised.value) == {
        "phase": "validation",
        "condition": "resource_path_missing",
        "spec_paths": ("input.DM.path",),
        "requirement": "REQ-0785",
        "context": {"dataset": "DM", "path": "missing.csv"},
    }


def test_producer_link_with_inline_types_reports_redundant_type(
    tmp_path: Path,
) -> None:
    (tmp_path / "dm.csv").write_text("ID\n001\n")

    with pytest.raises(SourceError) as raised:
        load_source_table(
            "DM",
            DatasetSource(
                path="dm.csv",
                schema="dm.schema.yaml",
                types={"ID": "str"},
            ),
            ProjectResources(tmp_path),
        )

    assert _diagnostic(raised.value) == {
        "phase": "validation",
        "condition": "redundant_field_type",
        "spec_paths": ("input.DM.types.ID",),
        "requirement": "REQ-0523",
        "context": {"dataset": "DM", "field": "ID", "type": "str"},
    }


def test_changed_content_is_reported_at_ingest_with_written_path(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    path = tmp_path / "dm.csv"
    path.write_text("ID\n001\n")
    resources = ProjectResources(tmp_path)
    original_capture = resources.capture

    def capture_then_change(written_path: str):
        snapshot = original_capture(written_path)
        path.write_text("ID\n002\n")
        return snapshot

    monkeypatch.setattr(resources, "capture", capture_then_change)
    with pytest.raises(SourceError) as raised:
        load_source_table("DM", DatasetSource(path="dm.csv"), resources)

    assert _diagnostic(raised.value) == {
        "phase": "ingest",
        "condition": "resource_path_content_changed",
        "spec_paths": ("input.DM.path",),
        "requirement": None,
        "context": {"dataset": "DM", "path": "dm.csv"},
    }
