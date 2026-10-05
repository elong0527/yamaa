"""Trusted compiler ports for bound reference metadata, without data or callback authority."""

from collections.abc import Callable, Collection, Mapping
from dataclasses import dataclass
from typing import TYPE_CHECKING, Literal, Protocol

from yamaa.models import ColumnType
from yamaa.odm.bindings import BindingPlan, BindingResult

if TYPE_CHECKING:
    from yamaa.specification.models import Expression

ComparableType = ColumnType | Literal["bool"]


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


@dataclass(frozen=True)
class IntermediateScope:
    """A normalized intermediate's source, field visibility and ordered dependencies."""

    dataset: str
    self_fields: tuple[str, ...]
    derived: tuple[str, ...]
    readable: tuple[str, ...]
    dependencies: tuple[str, ...]


@dataclass(frozen=True)
class IntermediateReadScope:
    """One correlated donor read with the fields available at its authored position."""

    reader: str
    target_name: str
    target: IntermediateScope | None
    field: str
    donor_dataset: str
    visible: tuple[str, ...]


@dataclass(frozen=True)
class IntermediateFinding:
    """A core visibility/phase finding or index of an unavailable donor dependency."""

    kind: str
    dependency: int | None = None


@dataclass(frozen=True)
class KeyInference:
    """Core-selected join keys or one failure, translated back to authored names."""

    kind: str
    keys: tuple[str, ...] = ()
    key: str | None = None
    expected: ColumnType | None = None
    actual: ColumnType | None = None


class ReferenceCompiler(Protocol):
    """An immutable catalog captured for one planning attempt, never a user extension hook."""

    def match_value_type(self, expression: "Expression") -> ComparableType | None:
        """Select the closed REQ-1259 comparison class or defer explicitly to runtime."""
        ...

    def comparable_types(self, left: ComparableType, right: ComparableType) -> bool:
        """Compare known types, including expression booleans, without coercing values."""
        ...

    def infer_keys(
        self, keys: tuple[str, ...], fields: Mapping[str, ColumnType]
    ) -> KeyInference:
        """Select applicable keys in output order against normalized right-side fields."""
        ...

    def bind(self, name: str) -> BindingResult:
        """Resolve one exact name without reading records."""
        ...

    def validate_intermediate(
        self, field: str, target: IntermediateScope
    ) -> tuple[IntermediateFinding, ...]:
        """Check declared intermediate field visibility without evaluating the relation."""
        ...

    def validate_intermediate_read(
        self, read: IntermediateReadScope
    ) -> tuple[IntermediateFinding, ...]:
        """Check donor scope and SELF phase before recursive intermediate execution."""
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
