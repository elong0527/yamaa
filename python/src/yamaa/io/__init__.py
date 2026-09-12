"""Secondary input/output adapters behind one common contract.

Each format lives in its own module: ``csv`` today, ``xpt`` and
``datajson`` next. Host-table treatment lives in ``polars``. Every
adapter delivers plain text-or-missing values; quoting is a transport
detail no adapter preserves.

The same modules write back out: ``csv.render_csv`` for the exact R020
csv bytes, ``polars.write_parquet_bytes`` for the Parquet profile, and
``publish`` for the atomic step from complete bytes to visible artifact.
"""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import PurePath

from yamaa.io.csv_write import render_csv
from yamaa.io.polars import write_parquet_bytes
from yamaa.io.project import ProjectResources, ResourceFailure
from yamaa.io.publish import ArtifactError, publish_artifact
from yamaa.io.source import (
    LoadedDataset,
    SourceDiagnostic,
    SourceError,
    load_source_table,
    load_source_tables,
)
from yamaa.models import TypedTable
from yamaa.specification.models import Output

__all__ = [
    "ArtifactError",
    "LoadedDataset",
    "ProjectResources",
    "ResourceFailure",
    "SourceDiagnostic",
    "SourceError",
    "load_source_table",
    "load_source_tables",
    "publish_artifact",
    "write_artifact",
]


def write_artifact(table: TypedTable, output: Output, keys: Sequence[str]) -> bytes:
    """Render one output declaration to its profile's complete bytes."""
    profile = PurePath(output.path).suffix.lower()
    if profile == ".csv":
        return render_csv(table, output.columns, keys, output.decimals)
    if profile == ".parquet":
        if output.decimals is not None:
            raise ArtifactError("decimals_not_applicable", output.decimals)
        return write_parquet_bytes(table, output.columns)
    raise ArtifactError("unknown_artifact_profile", output.path)
