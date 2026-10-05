"""Numeric syntax port shared by planning and optional backend admission."""

from collections.abc import Callable
from dataclasses import dataclass

from yamaa.expressions.numeric import (
    NumericAst,
    numeric_identifiers,
    parse_numeric_cached,
)


@dataclass(frozen=True)
class NumericSyntax:
    """One parsed expression and its ordered reads; consumers never mutate the AST."""

    ast: NumericAst
    identifiers: tuple[str, ...]


NumericAnalyzer = Callable[[str], NumericSyntax]


def analyze_numeric(text: str) -> NumericSyntax:
    """Retain the default backend's existing parser and metadata rules."""
    ast = parse_numeric_cached(text)
    return NumericSyntax(
        ast,
        numeric_identifiers(ast),
    )
