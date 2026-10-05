"""Name/index translation for the shared graph compiler; no host graph algorithms."""

import json

from yamaa.planning.dependencies import DependencyAnalysis, DependencyAnalyzer


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
