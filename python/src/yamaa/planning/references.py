"""Trusted compiler ports for bound reference metadata, without data or callback authority."""

from collections.abc import Callable, Collection, Mapping
from dataclasses import dataclass
from typing import Literal, Protocol

from yamaa.models import ColumnType
from yamaa.odm.bindings import BindingPlan, BindingResult


@dataclass(frozen=True)
class ReferenceFinding:
    """Core-selected condition with host names, before authored path attachment."""

    condition: str
    context: Mapping[str, str]


@dataclass(frozen=True)
class QualifiedScope:
    """Normalized direct-field context; selecting phases and joins remains host-owned."""

    drivers: tuple[str, ...]
    current_driver: bool
    reach: Literal["scalar", "record", "relation", "declared"]
    joined: bool
    phase: Literal["row", "column"]
    group_by: tuple[str, ...] | None = None
    groups: tuple[tuple[str, ...], ...] = ()


@dataclass(frozen=True)
class QualifiedFinding:
    """Core-selected scope finding, awaiting authored diagnostic context."""

    kind: str
    expected: ColumnType | None = None
    actual: ColumnType | None = None


class ReferenceCompiler(Protocol):
    """An immutable catalog captured for one planning attempt, never a user extension hook."""

    def bind(self, name: str) -> BindingResult:
        """Resolve one exact name without reading records."""
        ...

    def validate_qualified(
        self, name: str, expected: ColumnType | None, scope: QualifiedScope
    ) -> tuple[QualifiedFinding, ...]:
        """Select direct-field driver, existence, grouping, phase and type findings."""
        ...

    def validate_output(
        self,
        name: str,
        expected: ColumnType | None,
        available: Collection[str] | None,
        candidates: Collection[str | None],
    ) -> ReferenceFinding | None:
        """Validate a bare output reference in its already-selected phase."""
        ...


ReferenceCompilerFactory = Callable[
    [BindingPlan, Mapping[str, ColumnType]], ReferenceCompiler
]
