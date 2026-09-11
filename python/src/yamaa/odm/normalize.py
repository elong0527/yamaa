"""Polars-only normalization and atomic publication of one Parquet dataset."""

from __future__ import annotations

import hashlib
import os
import tempfile
from collections.abc import Iterable
from pathlib import Path

import polars as pl
import pyarrow.parquet as pq
from yamaa.odm.errors import ODMError
from yamaa.odm.profiles import ImportProfile, get_profile
from yamaa.odm.readers import iter_odm_records, resolve_archive_member
from yamaa.odm.schema import (
    ODM_ITEM_SCHEMA,
    SCHEMA_VERSION,
    ClinicalItemRow,
    NormalizationResult,
)

_PARQUET_COMPRESSIONS = {
    "brotli",
    "gzip",
    "lz4",
    "snappy",
    "uncompressed",
    "zstd",
}


def _sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _frame(records: Iterable[ClinicalItemRow]) -> pl.DataFrame:
    rows = [record.polars_row() for record in records]
    return pl.DataFrame(
        rows,
        schema=ODM_ITEM_SCHEMA,
        orient="row",
        strict=True,
    ).select(ODM_ITEM_SCHEMA.names())


def normalize_odm(
    source: str | Path,
    output: str | Path,
    *,
    profile: str | ImportProfile = "strict",
    batch_size: int = 10_000,
    compression: str = "zstd",
    overwrite: bool = False,
) -> NormalizationResult:
    """Normalize one ODM input into one atomically published Parquet file.

    Bounded Pydantic record batches are converted to Polars frames and handed
    to an incremental Parquet encoder. The encoder writes one staged Parquet
    file that is validated with Polars and atomically published. No CSV or
    intermediate dataset is created.
    """
    source_path = Path(source)
    output_path = Path(output)
    if not source_path.is_file():
        raise FileNotFoundError(source_path)
    if output_path.suffix.casefold() != ".parquet":
        raise ODMError("ODM normalization output must end in .parquet")
    if source_path.resolve() == output_path.resolve():
        raise ODMError("source and output paths must be different")
    if output_path.exists() and not overwrite:
        raise FileExistsError(output_path)
    if batch_size < 1:
        raise ODMError("batch_size must be at least 1")
    if compression not in _PARQUET_COMPRESSIONS:
        choices = ", ".join(sorted(_PARQUET_COMPRESSIONS))
        raise ODMError(f"unsupported Parquet compression {compression!r}: {choices}")

    selected_profile = get_profile(profile)
    archive_member = resolve_archive_member(source_path, selected_profile)
    source_sha256 = _sha256(source_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    count = 0
    batch: list[ClinicalItemRow] = []
    with tempfile.TemporaryDirectory(
        dir=output_path.parent,
        prefix=f".{output_path.name}.building-",
    ) as temporary_directory:
        temporary_path = Path(temporary_directory)
        staged_output = temporary_path / output_path.name
        parquet_metadata = {
            "yamaa.schema": SCHEMA_VERSION,
            "yamaa.profile": selected_profile.name,
            "yamaa.source_name": source_path.name,
            "yamaa.source_sha256": source_sha256,
        }
        if archive_member is not None:
            parquet_metadata["yamaa.archive_member"] = archive_member
        arrow_metadata = {
            key.encode("utf-8"): value.encode("utf-8")
            for key, value in parquet_metadata.items()
        }
        writer: pq.ParquetWriter | None = None

        def write_batch(records: list[ClinicalItemRow]) -> None:
            nonlocal writer
            table = _frame(records).to_arrow().replace_schema_metadata(arrow_metadata)
            if writer is None:
                arrow_compression = (
                    None if compression == "uncompressed" else compression
                )
                writer = pq.ParquetWriter(
                    staged_output,
                    table.schema,
                    compression=arrow_compression,
                    write_statistics=True,
                )
            writer.write_table(table, row_group_size=batch_size)

        try:
            for record in iter_odm_records(source_path, selected_profile):
                batch.append(record)
                count += 1
                if len(batch) == batch_size:
                    write_batch(batch)
                    batch.clear()

            if batch:
                write_batch(batch)
                batch.clear()
            elif writer is None:
                write_batch(batch)
            assert writer is not None
            writer.add_key_value_metadata({"yamaa.row_count": str(count)})
        finally:
            if writer is not None:
                writer.close()

        observed = (
            pl.scan_parquet(staged_output)
            .select(
                pl.len().alias("rows"),
                pl.col("SourceOrdinal").min().alias("first"),
                pl.col("SourceOrdinal").max().alias("last"),
                pl.col("SourceOrdinal").n_unique().alias("unique"),
                (pl.col("SourceOrdinal").diff().drop_nulls() != 1)
                .sum()
                .alias("order_breaks"),
            )
            .collect()
            .row(0, named=True)
        )
        expected_first = 1 if count else None
        expected_last = count if count else None
        if observed != {
            "rows": count,
            "first": expected_first,
            "last": expected_last,
            "unique": count,
            "order_breaks": 0,
        }:
            raise ODMError(f"staged Parquet validation failed: {observed}")

        os.replace(staged_output, output_path)

    return NormalizationResult(
        output_path=output_path,
        row_count=count,
        source_sha256=source_sha256,
        output_sha256=_sha256(output_path),
        profile=selected_profile.name,
        archive_member=archive_member,
    )
