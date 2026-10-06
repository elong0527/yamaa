"""Capture shared predicate syntax without a host parser or regex fallback."""

import json
from functools import lru_cache

from yamaa.expressions import PredicateError
from yamaa.planning.predicate_syntax import PredicateAnalyzer, PredicateSyntax


class NativePredicateLimitError(RuntimeError):
    """A parser/compiler resource refusal is not a language diagnostic."""

    def __init__(self, outcome):
        """Retain the exact accounting phase, resource, limit and source coordinates."""
        self.phase = outcome["phase"]
        self.resource = outcome["resource"]
        self.limit = outcome["limit"]
        self.position = dict(outcome["position"])
        super().__init__(
            f"native predicate {self.phase} {self.resource} limit exceeded"
        )


class NativePredicateUnsupportedError(RuntimeError):
    """An explicitly unsupported regex feature is separate from invalid grammar."""

    def __init__(self, outcome):
        """Preserve the compiler's feature and both predicate/pattern locations."""
        self.feature = outcome["feature"]
        self.pattern_byte = outcome["pattern_byte"]
        self.position = dict(outcome["position"])
        super().__init__(f"unsupported native predicate regex feature: {self.feature}")


def bind_predicate_analyzer(native) -> PredicateAnalyzer:
    """Capture a service before activation or IO and cache only within this run."""
    invoke = getattr(native, "analyze_predicate", None)
    if not callable(invoke):
        raise TypeError("native analyze_predicate must be callable")

    @lru_cache(maxsize=512)
    def analyze(text):
        """Decode Rust-owned syntax and preserve the failure category without fallback."""
        response = json.loads(
            invoke(
                json.dumps(
                    {"protocol": "predicate-syntax/1", "expression": text},
                    separators=(",", ":"),
                )
            )
        )
        if response["protocol"] != "predicate-syntax/1":
            raise ValueError("unsupported native predicate syntax response protocol")
        outcome = response["outcome"]
        if outcome["status"] == "resource_limit":
            raise NativePredicateLimitError(outcome)
        if outcome["status"] == "unsupported":
            raise NativePredicateUnsupportedError(outcome)
        if outcome["status"] == "invalid":
            message = outcome.get("message")
            if not isinstance(message, str) or not message:
                raise ValueError(
                    "native predicate syntax response lacks grammar message"
                )
            if message == "unexpected character":
                # Rendering a parser-identified scalar uses the existing host
                # display convention; the adapter never scans or parses input.
                message += f" {text[outcome['position']['character']]!r}"
            error = PredicateError(
                message,
                outcome["position"]["character"],
                outcome["requirement"],
            )
            error.condition = outcome["condition"]
            error.native_context = dict(outcome["context"])
            error.native_position = dict(outcome["position"])
            raise error
        if outcome["status"] != "parsed":
            raise ValueError("unknown native predicate syntax status")
        return PredicateSyntax(outcome["ast"], tuple(outcome["identifiers"]))

    return analyze
