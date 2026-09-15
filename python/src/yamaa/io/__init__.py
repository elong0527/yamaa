"""Secondary input/output adapters behind one common contract.

Each format lives in its own module. Host-table treatment lives in
``polars``. The ``csv`` adapter delivers text-or-missing values and the
``parquet`` adapter delivers the typed values in its embedded schema.
Quoting is a CSV transport detail no adapter preserves.

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
from yamaa.io.project import (
    PROJECT_CONFIGURATION_NAME,
    ApprovedRoots,
    ProjectConfigurationError,
    ProjectResources,
    ResourceFailure,
    approve_roots,
    find_project_configuration,
)
from yamaa.io.publish import ArtifactTarget, publish_artifact
from yamaa.io.source import (
    LoadedDataset,
    ProducerContract,
    ProducerField,
    ProducerSchemaUnresolved,
    SourceDiagnostic,
    SourceError,
    load_source_table,
    load_source_tables,
)

__all__ = [
    "PROJECT_CONFIGURATION_NAME",
    "ApprovedRoots",
    "Artifact",
    "ArtifactDiagnostic",
    "ArtifactError",
    "ArtifactProfile",
    "ArtifactTarget",
    "LoadedDataset",
    "ProducerContract",
    "ProducerField",
    "ProducerSchemaUnresolved",
    "ProjectConfigurationError",
    "ProjectResources",
    "ResourceFailure",
    "SourceDiagnostic",
    "SourceError",
    "approve_roots",
    "artifact_profile",
    "build_artifact",
    "find_project_configuration",
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
