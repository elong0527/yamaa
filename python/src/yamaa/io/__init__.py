"""Secondary input/output adapters behind one common contract.

Each format lives in its own module: ``csv`` today, ``xpt`` and
``datajson`` next. Host-table treatment lives in ``polars``. Every
adapter delivers plain text-or-missing values; quoting is a transport
detail no adapter preserves.

The same modules write the other way. ``artifact`` applies R005's column
selection and row order to a completed table, refuses what R020 cannot
write, and renders it through ``csv`` or ``parquet``; ``publish`` takes
the complete bytes from there to a visible artifact in one atomic step.
"""

from yamaa.io.artifact import (
    Artifact,
    ArtifactDiagnostic,
    ArtifactError,
    ArtifactProfile,
    artifact_profile,
    build_artifact,
    profile_of,
    render_artifact,
    render_csv,
)
from yamaa.io.parquet import parquet_schema, read_parquet, render_parquet
from yamaa.io.project import ProjectResources, ResourceFailure
from yamaa.io.publish import ArtifactTarget, publish_artifact
from yamaa.io.source import (
    LoadedDataset,
    SourceDiagnostic,
    SourceError,
    load_source_table,
    load_source_tables,
)

__all__ = [
    "Artifact",
    "ArtifactDiagnostic",
    "ArtifactError",
    "ArtifactProfile",
    "ArtifactTarget",
    "LoadedDataset",
    "ProjectResources",
    "ResourceFailure",
    "SourceDiagnostic",
    "SourceError",
    "artifact_profile",
    "build_artifact",
    "load_source_table",
    "load_source_tables",
    "parquet_schema",
    "profile_of",
    "publish_artifact",
    "read_parquet",
    "render_artifact",
    "render_csv",
    "render_parquet",
]
