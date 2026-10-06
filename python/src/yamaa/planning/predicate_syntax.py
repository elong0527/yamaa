"""Predicate syntax port shared by planning and optional backend admission."""

from collections.abc import Callable
from dataclasses import dataclass

from yamaa.expressions import PredicateAst, parse_predicate, predicate_identifiers


@dataclass(frozen=True)
class PredicateSyntax:
    """One parsed expression and ordered reads; consumers never mutate the AST."""

    ast: PredicateAst
    identifiers: tuple[str, ...]


PredicateAnalyzer = Callable[[str], PredicateSyntax]


def analyze_predicate(text: str) -> PredicateSyntax:
    """Keep the default backend's existing syntax and identifier rules."""
    ast = parse_predicate(text)
    return PredicateSyntax(ast, predicate_identifiers(ast))
