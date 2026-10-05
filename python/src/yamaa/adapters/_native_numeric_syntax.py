"""Translate the shared numeric compiler response without host parsing or fallback."""

import json
from functools import lru_cache

from yamaa.expressions.numeric import NumericError
from yamaa.planning.numeric_syntax import NumericAnalyzer, NumericSyntax


class NativeNumericLimitError(RuntimeError):
    """A compiler resource policy is separate from a language diagnostic."""

    def __init__(self, outcome):
        """Retain the exact limit and both native source coordinates."""
        self.resource = outcome["resource"]
        self.limit = outcome["limit"]
        self.position = dict(outcome["position"])
        super().__init__(f"native numeric syntax {self.resource} limit exceeded")


def bind_numeric_analyzer(native) -> NumericAnalyzer:
    """Capture one native service before activation or IO, with a bounded run-local cache."""
    invoke = getattr(native, "analyze_numeric", None)
    if not callable(invoke):
        raise TypeError("native analyze_numeric must be callable")

    @lru_cache(maxsize=512)
    def analyze(text):
        """Decode trusted native metadata; grammar errors retain their owning requirement."""
        response = json.loads(
            invoke(
                json.dumps(
                    {"protocol": "numeric-syntax/1", "expression": text},
                    separators=(",", ":"),
                )
            )
        )
        if response["protocol"] != "numeric-syntax/1":
            raise ValueError("unsupported native numeric syntax response protocol")
        outcome = response["outcome"]
        if outcome["status"] == "resource_limit":
            raise NativeNumericLimitError(outcome)
        if outcome["status"] == "invalid":
            raise NumericError(
                "invalid native numeric syntax",
                outcome["position"]["character"],
                condition=outcome["condition"],
                requirement=outcome["requirement"],
                context=outcome["context"],
            )
        if outcome["status"] != "parsed":
            raise ValueError("unknown native numeric syntax status")
        return NumericSyntax(
            outcome["ast"],
            tuple(outcome["identifiers"]),
        )

    return analyze
