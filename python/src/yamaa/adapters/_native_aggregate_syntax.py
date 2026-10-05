"""Translate the shared aggregate compiler response without host parsing or fallback."""

import json
from functools import lru_cache

from yamaa.expressions.aggregate import AggregateError
from yamaa.planning.aggregate_syntax import AggregateAnalyzer, AggregateSyntax


class NativeAggregateLimitError(RuntimeError):
    """A compiler resource policy is separate from a language diagnostic."""

    def __init__(self, outcome):
        """Retain the exact limit and both native source coordinates."""
        self.resource = outcome["resource"]
        self.limit = outcome["limit"]
        self.position = dict(outcome["position"])
        super().__init__(f"native aggregate syntax {self.resource} limit exceeded")


def bind_aggregate_analyzer(native) -> AggregateAnalyzer:
    """Capture one native service before activation or IO, with a bounded run-local cache."""
    invoke = getattr(native, "analyze_aggregate", None)
    if not callable(invoke):
        raise TypeError("native analyze_aggregate must be callable")

    @lru_cache(maxsize=512)
    def analyze(text):
        """Decode trusted native metadata; grammar errors retain their owning requirement."""
        response = json.loads(
            invoke(
                json.dumps(
                    {"protocol": "aggregate-syntax/1", "expression": text},
                    separators=(",", ":"),
                )
            )
        )
        if response["protocol"] != "aggregate-syntax/1":
            raise ValueError("unsupported native aggregate syntax response protocol")
        outcome = response["outcome"]
        if outcome["status"] == "resource_limit":
            raise NativeAggregateLimitError(outcome)
        if outcome["status"] == "invalid":
            raise AggregateError(
                "invalid native aggregate syntax",
                outcome["position"]["character"],
                condition=outcome["condition"],
                requirement=outcome["requirement"],
                context=outcome["context"],
            )
        if outcome["status"] != "parsed":
            raise ValueError("unknown native aggregate syntax status")
        return AggregateSyntax(
            outcome["ast"],
            tuple(outcome["identifiers"]),
            tuple(outcome["star_datasets"]),
            tuple(outcome["ungrouped_identifiers"]),
        )

    return analyze
