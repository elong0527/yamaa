"""Shared dataset profile selection from a written path."""

from __future__ import annotations

from pathlib import PurePosixPath
from typing import Literal, TypeAlias

DatasetProfile: TypeAlias = Literal["csv", "parquet"]

# R020-2 and R023-1 intentionally use the same closed extension mapping so
# an artifact can be consumed under the profile that produced it.
DATASET_PROFILES: dict[str, DatasetProfile] = {
    ".csv": "csv",
    ".parquet": "parquet",
}


def profile_of(path: str) -> DatasetProfile | None:
    """Return the dataset profile selected by a written path, if any."""
    return DATASET_PROFILES.get(PurePosixPath(path).suffix.lower())
