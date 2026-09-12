"""Stable failure identities for building and publishing an artifact."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue

# R020-46 reports the offending rows by key. The count beside them is the
# whole count, so a bound on how many are printed never changes the failure.
REPORTED_KEYS = 5


class ArtifactDiagnostic(BaseModel):
    """One artifact declaration or serialization failure."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    phase: Literal["validation", "output"]
    condition: str = Field(min_length=1)
    spec_paths: tuple[str, ...] = Field(min_length=1)
    requirement: str = Field(pattern=r"^R[0-9]{3}-[0-9]+$")
    context: dict[str, JsonValue]


class ArtifactError(ValueError):
    """Raised when an output declaration or a completed table cannot be written."""

    def __init__(self, diagnostics: list[ArtifactDiagnostic]) -> None:
        if not diagnostics:
            raise ValueError("ArtifactError requires at least one diagnostic")
        self.diagnostics = tuple(diagnostics)
        conditions = ", ".join(item.condition for item in diagnostics)
        super().__init__(f"artifact failed: {conditions}")
