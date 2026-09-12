"""Build, render, and publish the artifact a specification produces.

`build_artifact` applies R005's column selection and row order to a
completed table and refuses anything R020 cannot write. `render_artifact`
turns that into the bytes its profile fixes, and `publish_artifact`
replaces one caller-permitted target with them in a single step.
"""

from yamaa.artifacts.csv import fixed_point, render_csv
from yamaa.artifacts.diagnostics import ArtifactDiagnostic, ArtifactError
from yamaa.artifacts.output import (
    Artifact,
    ArtifactProfile,
    artifact_profile,
    build_artifact,
    profile_of,
)
from yamaa.artifacts.parquet import parquet_schema, read_parquet, render_parquet
from yamaa.artifacts.publish import ArtifactTarget, publish_artifact, render_artifact

__all__ = [
    "Artifact",
    "ArtifactDiagnostic",
    "ArtifactError",
    "ArtifactProfile",
    "ArtifactTarget",
    "artifact_profile",
    "build_artifact",
    "fixed_point",
    "parquet_schema",
    "profile_of",
    "publish_artifact",
    "read_parquet",
    "render_artifact",
    "render_csv",
    "render_parquet",
]
