"""Shared dataset profile selection from a written path."""

from __future__ import annotations

from pathlib import PurePosixPath
from typing import Literal, TypeAlias

DatasetProfile: TypeAlias = Literal["csv", "parquet"]

# REQ-0716 and REQ-0830 intentionally use the same closed extension mapping so
# an artifact can be consumed under the profile that produced it. REQ-1234
# admits a container here only when it carries every value the language does,
# and REQ-1235 keeps SAS Transport out on that ground; REQ-1233 keeps
# Dataset-JSON out because a study document, not a specification, holds its
# identifiers. An extension this table omits is refused under REQ-0760 rather
# than named as a declined container.
DATASET_PROFILES: dict[str, DatasetProfile] = {
    ".csv": "csv",
    ".parquet": "parquet",
}


def profile_of(path: str) -> DatasetProfile | None:
    """Return the dataset profile selected by a written path, if any."""
    return DATASET_PROFILES.get(PurePosixPath(path).suffix.lower())
