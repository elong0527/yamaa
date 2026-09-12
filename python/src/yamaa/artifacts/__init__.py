"""R020 artifact output: profile dispatch, exact bytes, atomic publication."""

from __future__ import annotations

from collections.abc import Sequence
from pathlib import PurePath

from yamaa.artifacts.csv import ArtifactError, render_csv
from yamaa.artifacts.parquet import write_parquet_bytes
from yamaa.artifacts.publish import publish_artifact
from yamaa.models import TypedTable
from yamaa.specification.models import Output


def write_artifact(
    table: TypedTable,
    output: Output,
    keys: Sequence[str],
) -> bytes:
    """Render the artifact's columns to the exact bytes its profile fixes."""
    extension = PurePath(output.path).suffix.lower()
    if extension == ".csv":
        return render_csv(table, output.columns, keys, output.decimals)
    if extension == ".parquet":
        if output.decimals is not None:
            raise ArtifactError(
                "decimals_not_applicable",
                output.path,
                {"decimals": output.decimals},
            )
        return write_parquet_bytes(table, output.columns)
    raise ArtifactError("unknown_artifact_profile", output.path)


__all__ = [
    "ArtifactError",
    "publish_artifact",
    "render_csv",
    "write_artifact",
    "write_parquet_bytes",
]
