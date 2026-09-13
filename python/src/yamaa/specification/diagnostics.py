"""Structured validation failures for specification loading."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue


class ValidationDiagnostic(BaseModel):
    """Portable identity and context for one validation failure."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    phase: Literal["validation"] = "validation"
    condition: str = Field(min_length=1)
    spec_paths: tuple[str, ...] = Field(min_length=1)
    requirement: str | None = Field(default=None, pattern=r"^R[0-9]{3}-[0-9]+$")
    context: dict[str, JsonValue]


class SpecificationError(ValueError):
    """Raised when a YAML document cannot become a valid specification."""

    def __init__(self, diagnostics: list[ValidationDiagnostic]) -> None:
        if not diagnostics:
            raise ValueError("SpecificationError requires at least one diagnostic")
        self.diagnostics = tuple(diagnostics)
        conditions = ", ".join(item.condition for item in diagnostics)
        super().__init__(f"specification validation failed: {conditions}")
