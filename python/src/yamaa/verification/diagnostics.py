"""Stable failure identities for key validation and verifications."""

from __future__ import annotations

from typing import Literal

from pydantic import BaseModel, ConfigDict, Field, JsonValue

# R009-7 lets an implementation bound how many offending keys it prints so
# long as the bound never changes pass or fail. The count beside them is
# always the whole count.
REPORTED_KEYS = 5


class VerificationFailure(BaseModel):
    """One failed key check or verification, in the committed error shape."""

    model_config = ConfigDict(strict=True, extra="forbid", frozen=True)

    phase: Literal["output", "verification"]
    condition: str = Field(min_length=1)
    spec_paths: tuple[str, ...] = Field(min_length=1)
    requirement: str = Field(pattern=r"^R[0-9]{3}-[0-9]+$")
    context: dict[str, JsonValue]


class VerificationError(ValueError):
    """Raised when a completed table fails key validation or verification."""

    def __init__(self, failures: list[VerificationFailure]) -> None:
        if not failures:
            raise ValueError("VerificationError requires at least one failure")
        self.failures = tuple(failures)
        conditions = ", ".join(failure.condition for failure in failures)
        super().__init__(f"verification failed: {conditions}")


class DeclarationError(ValueError):
    """Raised for a declaration R009 requires the validation phase to reject.

    A verification whose own declaration is invalid asserts nothing, so this
    component refuses it rather than reporting its rows as data failures.
    The failure belongs to validation, which the repository validator and
    the specification loader own, so it carries no runtime condition.
    """

    def __init__(
        self,
        spec_path: str,
        requirement: str,
        reason: str,
        *,
        condition: str | None = None,
        context: dict[str, JsonValue] | None = None,
    ) -> None:
        self.spec_path = spec_path
        self.requirement = requirement
        self.reason = reason
        self.condition = condition
        self.context = context or {}
        super().__init__(f"{spec_path}: {reason} ({requirement})")
