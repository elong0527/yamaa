"""Aggregate syntax port shared by planning and optional backend admission."""

from collections.abc import Callable
from dataclasses import dataclass

from yamaa.expressions.aggregate import (
    AggregateAst,
    aggregate_identifiers,
    aggregate_star_datasets,
    parse_aggregate_cached,
    ungrouped_identifiers,
)


@dataclass(frozen=True)
class AggregateSyntax:
    """One parsed expression and its ordered reads; consumers never mutate the AST."""

    ast: AggregateAst
    identifiers: tuple[str, ...]
    star_datasets: tuple[str, ...]
    ungrouped_identifiers: tuple[str, ...]


AggregateAnalyzer = Callable[[str], AggregateSyntax]


def analyze_aggregate(text: str) -> AggregateSyntax:
    """Retain the default backend's existing parser and metadata rules."""
    ast = parse_aggregate_cached(text)
    return AggregateSyntax(
        ast,
        aggregate_identifiers(ast),
        aggregate_star_datasets(ast),
        ungrouped_identifiers(ast),
    )
