from __future__ import annotations

import datetime as dt
from pathlib import Path

import polars as pl
import pytest

from yamaa.io import SourceError, load_source_table, load_source_tables
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
    # R016 forms, but `date_impute` does. R016-32 makes collected precision
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


def test_adae_fixture_treats_bare_and_quoted_empty_as_missing() -> None:
    root = REPOSITORY / "yaml/examples/adam-adae-string-handlers"

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
    ("example", "dataset", "path", "condition"),
    [
        (
            "negative-dataset-path-absolute",
            "LBREF",
            "/shared/reference/lbref.csv",
            "resource_path_not_relative",
        ),
        (
            "negative-dataset-path-directory",
            "LBREF",
            "input/lbref",
            "resource_path_not_regular_file",
        ),
        (
            "negative-dataset-path-missing",
            "LBREF",
            "input/lbref.csv",
            "resource_path_missing",
        ),
        (
            "negative-dataset-path-parent-escape",
            "LBREF",
            "../reference/lbref.csv",
            "resource_path_outside_project",
        ),
        (
            "negative-dataset-path-symlink",
            "LBREF",
            "input/lbref.csv",
            "resource_path_symlink",
        ),
        (
            "negative-dataset-path-url",
            "LBREF",
            "https://reference.example.org/limits/lbref.csv",
            "resource_path_uri_scheme",
        ),
    ],
)
def test_path_fixtures_report_exact_diagnostics(
    example: str,
    dataset: str,
    path: str,
    condition: str,
) -> None:
    root = REPOSITORY / "yaml" / "examples" / example
    with pytest.raises(SourceError) as raised:
        load_source_table(
            dataset,
            DatasetSource(path=path),
            ProjectResources(root),
        )

    assert _diagnostic(raised.value) == {
        "phase": "validation",
        "condition": condition,
        "spec_paths": (f"datasets.{dataset}.path",),
        "context": {"dataset": dataset, "path": path},
    }


def test_committed_symlink_fixture_is_a_real_symlink() -> None:
    path = REPOSITORY / "yaml/examples/negative-dataset-path-symlink/input/lbref.csv"
    assert path.is_symlink()


@pytest.mark.parametrize(
    ("example", "condition", "context"),
    [
        (
            "negative-source-duplicate-field-name",
            "source_field_name_duplicate",
            {"record": 1, "field": "SEX"},
        ),
        (
            "negative-source-empty-field-name",
            "source_field_name_empty",
            {"record": 1, "field": 4},
        ),
        (
            "negative-source-invalid-text",
            "invalid_text",
            {"record": 3, "field": 3},
        ),
        (
            "negative-source-record-width",
            "source_record_width",
            {"record": 3, "field": 5},
        ),
        (
            "negative-source-unterminated-quote",
            "source_quote_unterminated",
            {"record": 3, "field": 3},
        ),
    ],
)
def test_csv_fixtures_report_exact_diagnostics(
    example: str, condition: str, context: dict[str, object]
) -> None:
    root = REPOSITORY / "yaml" / "examples" / example
    path = "input/dm.csv"
    with pytest.raises(SourceError) as raised:
        load_source_table("DM", DatasetSource(path=path), ProjectResources(root))

    assert _diagnostic(raised.value) == {
        "phase": "ingest",
        "condition": condition,
        "spec_paths": ("datasets.DM.path",),
        "context": {"dataset": "DM", "path": path, **context},
    }


def test_unknown_profile_fails_before_snapshot_bytes_are_read() -> None:
    root = REPOSITORY / "yaml/examples/negative-source-unknown-profile"
    resources = ProjectResources(root)

    with pytest.raises(SourceError) as raised:
        load_source_table("DM", DatasetSource(path="input/dm.txt"), resources)

    assert _diagnostic(raised.value) == {
        "phase": "validation",
        "condition": "source_profile_unknown",
        "spec_paths": ("datasets.DM.path",),
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
        ("negative-source-missing-sentinel", "AGE", "int", "NA"),
        ("negative-ingest-unparseable-field", "EXDOSE", "float", "200 mg"),
    ],
)
def test_typed_parse_fixtures_are_ingestion_failures(
    example: str, field: str, target: str, value: str
) -> None:
    root = REPOSITORY / "yaml" / "examples" / example
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
        "spec_paths": (f"datasets.{dataset}.types.{field}",),
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
        "spec_paths": ("datasets.DM.types.AGEYRS",),
        "context": {"dataset": "DM", "field": "AGEYRS"},
    }


def test_producer_link_requires_workflow_resolution(tmp_path: Path) -> None:
    (tmp_path / "dm.csv").write_text("ID\n001\n")

    with pytest.raises(NotImplementedError, match="workflow resolution"):
        load_source_table(
            "DM",
            DatasetSource(path="dm.csv", schema="dm.schema.yaml"),
            ProjectResources(tmp_path),
        )


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
        "spec_paths": ("datasets.DM.path",),
        "context": {"dataset": "DM", "path": "dm.csv"},
    }
