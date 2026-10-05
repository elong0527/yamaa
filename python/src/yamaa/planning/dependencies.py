"""Trusted compiler port for bound graph analysis; no specification extension hook."""

from collections.abc import Callable, Collection, Mapping, Sequence
from dataclasses import dataclass


@dataclass(frozen=True)
class DependencyAnalysis:
    """Portable declaration names for a cycle and the maximal stable scheduling order."""

    cycle: tuple[str, ...] | None
    order: tuple[str, ...]


DependencyAnalyzer = Callable[
    [Sequence[str], Mapping[str, Collection[str]]], DependencyAnalysis
]
