"""Trusted compiler ports for bound reference metadata, without data or callback authority."""

from collections.abc import Callable, Collection, Mapping
from dataclasses import dataclass
from typing import Protocol

from yamaa.models import ColumnType
from yamaa.odm.bindings import BindingPlan, BindingResult


@dataclass(frozen=True)
class ReferenceFinding:
    """Core-selected condition with host names, before authored path attachment."""

    condition: str
    context: Mapping[str, str]


class ReferenceCompiler(Protocol):
    """An immutable catalog captured for one planning attempt, never a user extension hook."""

    def bind(self, name: str) -> BindingResult:
        """Resolve one exact name without reading records."""
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
