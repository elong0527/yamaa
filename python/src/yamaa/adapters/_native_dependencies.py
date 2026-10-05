"""Name/index translation for the shared graph compiler; no host graph algorithms."""

import json

from yamaa.planning.dependencies import (
    ColumnDependencyAnalysis,
    ColumnDependencyAnalyzer,
    ColumnDependencyDiagnostic,
    DependencyAnalysis,
    DependencyAnalyzer,
)


class NativeDependencyLimitError(RuntimeError):
    """A native compiler resource policy, separate from a language cycle condition."""

    def __init__(self, outcome):
        """Retain exact graph work limits without pretending they are semantic diagnostics."""
        self.resource = outcome["resource"]
        self.limit = int(outcome["limit"])
        self.required = int(outcome["required"])
        super().__init__(f"native dependency analysis {self.resource} limit exceeded")


def bind_dependency_analyzer(native) -> DependencyAnalyzer:
    """Capture the selected native service before activation or source-provider effects."""
    invoke = getattr(native, "analyze_dependencies", None)
    if not callable(invoke):
        raise TypeError("native analyze_dependencies must be callable")

    def analyze(names, dependencies):
        """Translate only bindings; Rust owns cycle selection and scheduling order."""
        names = tuple(names)
        positions = {name: index for index, name in enumerate(names)}
        request = {
            "protocol": "dependency-analysis/1",
            "dependencies": [
                [
                    positions[dependency]
                    for dependency in dependencies.get(name, ())
                    if dependency in positions
                ]
                for name in names
            ],
        }
        response = json.loads(invoke(json.dumps(request, separators=(",", ":"))))
        if response["protocol"] != "dependency-analysis/1":
            raise ValueError("unsupported native dependency analysis response protocol")
        outcome = response["outcome"]
        if outcome["status"] == "limit":
            raise NativeDependencyLimitError(outcome)
        if outcome["status"] != "complete":
            raise ValueError("unknown native dependency analysis status")
        return DependencyAnalysis(
            cycle=None
            if outcome["cycle"] is None
            else tuple(names[index] for index in outcome["cycle"]),
            order=tuple(names[index] for index in outcome["order"]),
        )

    return analyze


def bind_column_dependency_analyzer(native) -> ColumnDependencyAnalyzer:
    """Capture the column rule service before activation and data-provider effects."""
    invoke = getattr(native, "analyze_column_dependencies", None)
    if not callable(invoke):
        raise TypeError("native analyze_column_dependencies must be callable")

    def analyze(names, dependencies, keys, has_rows):
        """Translate bound names and returned metadata without deciding language rules.

        A key naming no declared column cannot cross as an index, so it is
        reported here as the REQ-0074 `key_dependency` diagnostic the
        reference planner raises for it, rather than failing name lookup.
        """
        names = tuple(names)
        positions = {name: index for index, name in enumerate(names)}
        undeclared = [key for key in keys if key not in positions]
        request = {
            "protocol": "column-dependencies/1",
            "dependencies": [
                [
                    positions[dependency]
                    for dependency in dependencies[name]
                    if dependency in positions
                ]
                if name in dependencies
                else None
                for name in names
            ],
            "keys": [positions[key] for key in keys if key in positions],
            "has_rows": has_rows,
        }
        response = json.loads(invoke(json.dumps(request, separators=(",", ":"))))
        if response["protocol"] != "column-dependencies/1":
            raise ValueError("unsupported native column dependency response protocol")
        outcome = response["outcome"]
        if outcome["status"] == "limit":
            raise NativeDependencyLimitError(outcome)
        if outcome["status"] != "complete":
            raise ValueError("unknown native column dependency status")
        return ColumnDependencyAnalysis(
            order=tuple(names[index] for index in outcome["order"]),
            diagnostics=tuple(
                ColumnDependencyDiagnostic(
                    condition=item["condition"],
                    requirement=item["requirement"],
                    location=item["location"],
                    columns=tuple(names[index] for index in item["columns"]),
                )
                for item in outcome["diagnostics"]
            )
            + tuple(
                # The reference planner reports an undeclared key as
                # `key_dependency` after the shared analysis diagnostics;
                # row templates exempt keys the same way (REQ-0074).
                ColumnDependencyDiagnostic(
                    condition="key_dependency",
                    requirement="REQ-0074",
                    location="declaration",
                    columns=(key,),
                )
                for key in undeclared
                if not has_rows
            ),
        )

    return analyze
