"""The two shapes an R018 failure takes on its way to a run's diagnostics.

A failure is discovered where R018 places it -- resolving a root, verifying
an artifact, running a vector, invoking a binding -- and only the caller
knows which specification text demanded that work. `FunctionFailure` carries
the portable identity REQ-0694 through REQ-0703 fixes; `anchor` attaches the
specification paths, and `FunctionActivationError` carries the finished
diagnostics to the runner.
"""

from __future__ import annotations

from collections.abc import Sequence

from pydantic import JsonValue

from yamaa.models.values import ConditionPhase
from yamaa.planning import ExecutionDiagnostic

# REQ-0704: every failure names the specification text that required an
# implementation stage. A project root selected for a specification that
# calls no function anchors at the document root instead.
SPECIFICATION_ROOT = "$"


class FunctionFailure(ValueError):
    """One R018 failure identified before it is anchored to a call site."""

    def __init__(
        self,
        condition: str,
        requirement: str,
        context: dict[str, JsonValue],
        *,
        phase: ConditionPhase = "validation",
        spec_paths: Sequence[str] = (),
    ) -> None:
        self.condition = condition
        self.requirement = requirement
        self.context = dict(context)
        self.phase: ConditionPhase = phase
        self.spec_paths = tuple(spec_paths)
        super().__init__(f"{condition}: {requirement}")

    def anchor(self, spec_paths: Sequence[str]) -> ExecutionDiagnostic:
        """Return this failure reported against the paths that required it."""
        paths = self.spec_paths or tuple(spec_paths) or (SPECIFICATION_ROOT,)
        return ExecutionDiagnostic(
            phase=self.phase,
            condition=self.condition,
            spec_paths=paths,
            requirement=self.requirement,
            context=self.context,
        )


class FunctionActivationError(ValueError):
    """Raised when a project environment cannot be activated for a run."""

    def __init__(self, diagnostics: Sequence[ExecutionDiagnostic]) -> None:
        if not diagnostics:
            raise ValueError("FunctionActivationError requires at least one diagnostic")
        self.diagnostics = tuple(diagnostics)
        conditions = ", ".join(diagnostic.condition for diagnostic in diagnostics)
        super().__init__(f"project functions failed to activate: {conditions}")


__all__ = ["SPECIFICATION_ROOT", "FunctionActivationError", "FunctionFailure"]
