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

from importlib import import_module as _import_module
from typing import TYPE_CHECKING

if TYPE_CHECKING:
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

_EXPORTS = {
    "Artifact": ("yamaa.io.artifact", "Artifact"),
    "ArtifactDiagnostic": ("yamaa.io.artifact", "ArtifactDiagnostic"),
    "ArtifactError": ("yamaa.io.artifact", "ArtifactError"),
    "ArtifactProfile": ("yamaa.io.artifact", "ArtifactProfile"),
    "artifact_profile": ("yamaa.io.artifact", "artifact_profile"),
    "build_artifact": ("yamaa.io.artifact", "build_artifact"),
    "profile_of": ("yamaa.io.artifact", "profile_of"),
    "render_artifact": ("yamaa.io.artifact", "render_artifact"),
    "render_csv": ("yamaa.io.artifact", "render_csv"),
    "parquet_schema": ("yamaa.io.parquet", "parquet_schema"),
    "read_parquet": ("yamaa.io.parquet", "read_parquet"),
    "render_parquet": ("yamaa.io.parquet", "render_parquet"),
    "PROJECT_CONFIGURATION_NAME": ("yamaa.io.project", "PROJECT_CONFIGURATION_NAME"),
    "ApprovedRoots": ("yamaa.io.project", "ApprovedRoots"),
    "ProjectConfigurationError": ("yamaa.io.project", "ProjectConfigurationError"),
    "ProjectResources": ("yamaa.io.project", "ProjectResources"),
    "ResourceFailure": ("yamaa.io.project", "ResourceFailure"),
    "approve_roots": ("yamaa.io.project", "approve_roots"),
    "find_project_configuration": ("yamaa.io.project", "find_project_configuration"),
    "ArtifactTarget": ("yamaa.io.publish", "ArtifactTarget"),
    "publish_artifact": ("yamaa.io.publish", "publish_artifact"),
    "LoadedDataset": ("yamaa.io.source", "LoadedDataset"),
    "ProducerContract": ("yamaa.io.source", "ProducerContract"),
    "ProducerField": ("yamaa.io.source", "ProducerField"),
    "ProducerSchemaUnresolved": ("yamaa.io.source", "ProducerSchemaUnresolved"),
    "SourceDiagnostic": ("yamaa.io.source", "SourceDiagnostic"),
    "SourceError": ("yamaa.io.source", "SourceError"),
    "load_source_table": ("yamaa.io.source", "load_source_table"),
    "load_source_tables": ("yamaa.io.source", "load_source_tables"),
}


def __getattr__(name: str):
    """Load a reference export only when that export is requested."""
    if name not in _EXPORTS:
        raise AttributeError(f"module {__name__!r} has no attribute {name!r}")
    module, attribute = _EXPORTS[name]
    value = getattr(_import_module(module), attribute)
    globals()[name] = value
    return value


def __dir__() -> list[str]:
    return sorted(set(globals()) | set(_EXPORTS))


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
