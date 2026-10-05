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


@dataclass(frozen=True)
class ColumnDependencyDiagnostic:
    """Core-selected rule and authored path kind, before host path attachment."""

    condition: str
    requirement: str
    location: str
    columns: tuple[str, ...]


@dataclass(frozen=True)
class ColumnDependencyAnalysis:
    """Column schedule and ordered dependency-rule failures from the shared compiler."""

    order: tuple[str, ...]
    diagnostics: tuple[ColumnDependencyDiagnostic, ...]


ColumnDependencyAnalyzer = Callable[
    [Sequence[str], Mapping[str, Collection[str]], Sequence[str], bool],
    ColumnDependencyAnalysis,
]
