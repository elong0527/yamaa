"""Write ODM clinical items to one bounded-memory Parquet file."""

from __future__ import annotations

import os
import tempfile
from pathlib import Path

import pyarrow.parquet as pq
from yamaa.odm.errors import ODMError
from yamaa.odm.readers import iter_odm_records
from yamaa.odm.schema import (
    SCHEMA_VERSION,
    ClinicalItemRow,
    ParquetWriteResult,
    rows_to_frame,
)

_PARQUET_COMPRESSIONS = {
    "brotli",
    "gzip",
    "lz4",
    "snappy",
    "uncompressed",
    "zstd",
}


def write_odm_parquet(
    source: str | Path,
    output: str | Path,
    *,
    archive_member: str | None = None,
    batch_size: int = 10_000,
    compression: str = "zstd",
    overwrite: bool = False,
    max_expanded_bytes: int = 256 * 1024 * 1024,
    max_xml_depth: int = 64,
) -> ParquetWriteResult:
    """Atomically write one fixed-schema Parquet file from an ODM input."""
    source_path = Path(source)
    output_path = Path(output)
    if not source_path.is_file():
        raise FileNotFoundError(source_path)
    if output_path.suffix.casefold() != ".parquet":
        raise ODMError("output must end in .parquet")
    if source_path.resolve() == output_path.resolve():
        raise ODMError("source and output paths must be different")
    if output_path.exists() and not overwrite:
        raise FileExistsError(output_path)
    if batch_size < 1:
        raise ODMError("batch_size must be at least 1")
    if compression not in _PARQUET_COMPRESSIONS:
        choices = ", ".join(sorted(_PARQUET_COMPRESSIONS))
        raise ODMError(f"unsupported compression {compression!r}: {choices}")

    output_path.parent.mkdir(parents=True, exist_ok=True)
    row_count = 0
    batch: list[ClinicalItemRow] = []

    with tempfile.TemporaryDirectory(
        dir=output_path.parent,
        prefix=f".{output_path.name}.building-",
    ) as temporary_directory:
        staged_output = Path(temporary_directory) / output_path.name
        metadata = {b"yamaa.schema": SCHEMA_VERSION.encode("utf-8")}
        writer: pq.ParquetWriter | None = None

        def write_batch(records: list[ClinicalItemRow]) -> None:
            nonlocal writer
            table = rows_to_frame(records).to_arrow().replace_schema_metadata(metadata)
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
            for record in iter_odm_records(
                source_path,
                archive_member=archive_member,
                max_expanded_bytes=max_expanded_bytes,
                max_xml_depth=max_xml_depth,
            ):
                batch.append(record)
                row_count += 1
                if len(batch) == batch_size:
                    write_batch(batch)
                    batch.clear()

            if batch:
                write_batch(batch)
                batch.clear()
            elif writer is None:
                write_batch(batch)
            assert writer is not None
            writer.add_key_value_metadata({"yamaa.row_count": str(row_count)})
        finally:
            if writer is not None:
                writer.close()

        if overwrite:
            os.replace(staged_output, output_path)
        else:
            try:
                os.link(staged_output, output_path)
            except FileExistsError:
                raise FileExistsError(output_path) from None

    return ParquetWriteResult(output_path=output_path, row_count=row_count)
