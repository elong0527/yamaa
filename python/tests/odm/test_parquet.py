from __future__ import annotations

from pathlib import Path

import polars as pl
import pytest

from yamaa.odm.errors import ODMError
from yamaa.odm.parquet import write_odm_parquet
from yamaa.odm.schema import ODM_ITEM_SCHEMA


def test_writer_publishes_one_parquet_file(odm13_path: Path, tmp_path: Path) -> None:
    output_directory = tmp_path / "output"
    output = output_directory / "clinical-items.parquet"

    result = write_odm_parquet(odm13_path, output, batch_size=1)

    assert result.row_count == 3
    assert result.output_path == output
    assert sorted(path.name for path in output_directory.iterdir()) == [output.name]
    assert not list(output_directory.glob("*.csv"))

    frame = pl.read_parquet(output)
    assert frame.schema == ODM_ITEM_SCHEMA
    assert frame["SourceOrdinal"].to_list() == [1, 2, 3]
    assert frame["Value"].to_list() == ["2026-01-02", "", None]
    assert frame["ValuePresent"].to_list() == [True, True, False]
    assert frame["IsNull"].to_list() == [False, False, True]
    metadata = pl.read_parquet_metadata(output)
    assert metadata["yamaa.schema"] == "odm-clinical-item/1"
    assert metadata["yamaa.row_count"] == "3"


def test_writer_projects_two_groups_into_same_schema(
    odm20_path: Path, tmp_path: Path
) -> None:
    output = tmp_path / "odm20.parquet"
    write_odm_parquet(odm20_path, output, batch_size=1)

    frame = pl.read_parquet(output)
    assert frame.schema == ODM_ITEM_SCHEMA
    assert frame.select(
        "StudySubjectID",
        "FormOID",
        "FormRepeatKey",
        "ItemGroupOID",
        "ItemGroupRepeatKey",
        "ItemName",
    ).row(0) == (None, "FO.20", "2", "IG.20", "3", "Test name")


def test_output_must_be_parquet(odm13_path: Path, tmp_path: Path) -> None:
    with pytest.raises(ODMError, match="must end in .parquet"):
        write_odm_parquet(odm13_path, tmp_path / "clinical-items.csv")


def test_existing_output_requires_explicit_overwrite(
    odm13_path: Path, tmp_path: Path
) -> None:
    output = tmp_path / "clinical-items.parquet"
    output.write_bytes(b"existing")

    with pytest.raises(FileExistsError):
        write_odm_parquet(odm13_path, output)
    assert output.read_bytes() == b"existing"

    result = write_odm_parquet(odm13_path, output, overwrite=True)
    assert result.row_count == 3
    assert pl.read_parquet(output).height == 3


def test_late_arriving_destination_is_preserved(
    odm13_path: Path, tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    output = tmp_path / "clinical-items.parquet"
    output.write_bytes(b"racing")
    real_exists = Path.exists
    calls = 0

    def flaky_exists(self: Path) -> bool:
        nonlocal calls
        if self == output and calls == 0:
            calls += 1
            return False
        return real_exists(self)

    monkeypatch.setattr(Path, "exists", flaky_exists)
    with pytest.raises(FileExistsError):
        write_odm_parquet(odm13_path, output)
    assert output.read_bytes() == b"racing"


def test_empty_odm_still_publishes_fixed_schema(tmp_path: Path) -> None:
    source = tmp_path / "empty.xml"
    source.write_text(
        '<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3" ODMVersion="1.3"/>',
        encoding="utf-8",
    )
    output = tmp_path / "empty.parquet"

    result = write_odm_parquet(source, output)

    assert result.row_count == 0
    assert pl.read_parquet(output).is_empty()
    assert pl.read_parquet_schema(output) == ODM_ITEM_SCHEMA
