"""R003 intermediates: select one record once, then read it many times.

An intermediate states its match once and gives the chosen record a name, so the
columns that read it are plainly reading one record. Everything about
reaching that record -- filtering, equality matching, range narrowing, and
ordered selection -- is one explicit declared-key mechanism, so a named
intermediate and an inline `lookup:` cannot disagree.
"""

from __future__ import annotations

from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import Literal

from pydantic import JsonValue, ValidationError

from yamaa.expressions.core import (
    FailedResolution,
    Resolution,
    ResolvedValue,
    normalize_runtime_value,
)
from yamaa.expressions.predicates import PredicateError, parse_predicate_cached
from yamaa.models import (
    MISSING,
    ColumnType,
    ConditionResult,
    EvaluationResult,
    HandlerName,
    RuntimeCondition,
    RuntimeValue,
    ValueResult,
    runtime_type_name,
)
from yamaa.planning import PlannedIntermediate
from yamaa.runtime.joins import (
    IndexedRecord,
    RelationIndex,
    compare_values,
    eligible_records,
    json_value,
    select_record,
)
from yamaa.specification.models import OrderTerm


@dataclass(frozen=True, slots=True)
class IntermediateOutcome:
    """What one current row got from an intermediate.

    A selected record and a decided absence stay distinct: REQ-0124 keeps a
    matched record whose value is missing different from a match that never
    happened, which answers with the declared `missing` literal instead.
    """

    record: IndexedRecord | None = None
    absent: JsonValue = None
    condition: ConditionResult | None = None
    spec_path: str | None = None
    handled_by: HandlerName | None = None


def _condition(
    condition: str,
    requirement: str,
    context: Mapping[str, JsonValue],
    *,
    phase: Literal["validation", "join"] = "join",
) -> ConditionResult:
    return ConditionResult(
        condition=RuntimeCondition(
            phase=phase,
            condition=condition,
            context=dict(context),
            requirement=requirement,
        )
    )


def types_comparable(left: ColumnType, right: ColumnType) -> bool:
    """Return whether two declared types may be compared without conversion.

    REQ-0121 lets `int` and `float` compare through the Numeric values contract's
    promotion and requires every other type to match exactly, so no operand is
    converted implicitly to make a range comparison work.
    """
    return left == right or {left, right} <= {"int", "float"}


class IntermediateSelector:
    """Select at most one record per current row for each declared intermediate."""

    def __init__(
        self,
        plans: Sequence[PlannedIntermediate],
        relations: Mapping[str, RelationIndex],
    ) -> None:
        self.plans = {plan.identifier: plan for plan in plans}
        self._relations = relations
        self._eligible: dict[str, tuple[IndexedRecord, ...] | ConditionResult] = {}

    def declares(self, identifier: str) -> bool:
        return identifier in self.plans

    def _filtered(
        self, plan: PlannedIntermediate
    ) -> tuple[IndexedRecord, ...] | ConditionResult:
        """Apply the intermediate's `filter` once for the whole run.

        REQ-0120 makes the filter a predicate over the intermediate's own dataset,
        so which records are eligible does not vary by current row and the
        predicate is evaluated once per record rather than once per row.
        """
        cached = self._eligible.get(plan.identifier)
        if cached is None:
            relation = self._relations[plan.dataset]
            kept = eligible_records(relation.records, plan.filter_predicate, relation)
            cached = kept if isinstance(kept, ConditionResult) else tuple(kept)
            self._eligible[plan.identifier] = cached
        return cached

    def select(
        self,
        identifier: str,
        current: Mapping[str, RuntimeValue],
    ) -> IntermediateOutcome:
        """Choose this row's record, in the order R003 lays the steps out."""
        plan = self.plans[identifier]
        eligible = self._filtered(plan)
        if isinstance(eligible, ConditionResult):
            return IntermediateOutcome(
                condition=eligible, spec_path=f"{plan.path}.filter"
            )
        return _select_eligible(plan, eligible, current)


def _select_eligible(
    plan: PlannedIntermediate,
    eligible: Sequence[IndexedRecord],
    current: Mapping[str, RuntimeValue],
) -> IntermediateOutcome:
    """Match, narrow, and choose one record from the eligible records.

    Named and inline intermediates share these steps: the named selector caches
    the eligible records per intermediate, while an inline `lookup:` derives them
    from its payload on every row.
    """
    values = [current.get(name, MISSING) for name in plan.match_variables]
    if any(value is MISSING for value in values):
        # A missing match value is not an identity any record shares,
        # so the intermediate yields nothing before any record is read.
        return _absent(plan, values)
    if (
        plan.between_value is not None
        and current.get(plan.between_value, MISSING) is MISSING
    ):
        # A missing range value is incomplete, not unmatched: it yields
        # nothing before any record is read.
        return _absent(plan, values)
    matched = [
        record
        for record in eligible
        if all(
            _equal(record.values[field], value)
            for field, value in zip(plan.match_fields, values, strict=True)
        )
    ]

    narrowed = _narrowed(plan, matched, current)
    if isinstance(narrowed, IntermediateOutcome):
        return narrowed

    if not narrowed:
        return _absent(plan, values)
    if len(narrowed) == 1:
        return IntermediateOutcome(record=narrowed[0])
    if plan.keep is None:
        # REQ-0127: more than one surviving record with nothing to choose
        # by is the unhandled multiple match the rule refuses.
        return IntermediateOutcome(
            condition=_condition(
                "multiple_matches",
                "REQ-0127",
                {
                    "intermediate": plan.identifier,
                    "dataset": plan.dataset,
                    **_matched_key(plan, values),
                    "match_count": len(narrowed),
                },
            ),
            spec_path=plan.path,
        )
    chosen = select_record(narrowed, plan.order_terms, plan.keep)
    if isinstance(chosen, ConditionResult):
        return IntermediateOutcome(condition=chosen, spec_path=f"{plan.path}.order_by")
    handled_by: HandlerName | None = None
    if len(narrowed) > 1:
        # The declared keep actually chose among surviving records: R008
        # counts the selection where it happened.
        handled_by = "multiple_matches"
    return IntermediateOutcome(record=chosen, handled_by=handled_by)


def _absent(
    plan: PlannedIntermediate,
    values: Sequence[RuntimeValue],
) -> IntermediateOutcome:
    """Answer an intermediate that yields nothing under REQ-0124."""
    if plan.strict:
        return IntermediateOutcome(
            condition=_condition(
                "unmatched_key",
                "REQ-0124",
                {
                    "intermediate": plan.identifier,
                    "dataset": plan.dataset,
                    **_matched_key(plan, values),
                },
            ),
            spec_path=plan.path,
        )
    handled_by: HandlerName | None = "missing" if plan.missing_declared else None
    return IntermediateOutcome(absent=plan.missing, handled_by=handled_by)


def _narrowed(
    plan: PlannedIntermediate,
    matched: Sequence[IndexedRecord],
    current: Mapping[str, RuntimeValue],
) -> list[IndexedRecord] | IntermediateOutcome:
    """Keep the equality-matched records the declared range admits."""
    if plan.between_value is None:
        return list(matched)
    value = current.get(plan.between_value, MISSING)
    assert plan.between_lower is not None
    assert plan.between_upper is not None
    kept: list[IndexedRecord] = []
    for record in matched:
        lower = record.values[plan.between_lower]
        upper = record.values[plan.between_upper]
        # REQ-0128: both endpoints are inclusive, and a record missing a
        # stated bound is ineligible rather than open.
        if lower is MISSING or upper is MISSING:
            continue
        try:
            if compare_values(lower, value) <= 0 and compare_values(value, upper) <= 0:
                kept.append(record)
        except TypeError:
            return IntermediateOutcome(
                condition=_condition(
                    "incomparable_range_types",
                    "REQ-0121",
                    {
                        "intermediate": plan.identifier,
                        "value_type": runtime_type_name(value),
                        "lower_type": runtime_type_name(lower),
                        "upper_type": runtime_type_name(upper),
                    },
                    phase="validation",
                ),
                spec_path=f"{plan.path}.between",
            )
    return kept


def _matched_key(
    plan: PlannedIntermediate,
    values: Sequence[RuntimeValue],
) -> dict[str, JsonValue]:
    """Return the fields the intermediate matched on and the values it matched with.

    REQ-0143 keeps one vocabulary for every intermediate failure, so an unmatched
    key and an unhandled multiple match report the match the same way and
    leave `keys` to the output row the failure belongs to.
    """
    return {
        "key": list(plan.match_fields),
        "intermediate_key": {
            field: json_value(value)
            for field, value in zip(plan.match_fields, values, strict=True)
        },
    }


def _equal(left: RuntimeValue, right: RuntimeValue) -> bool:
    """Compare two match values under the equality their type owns.

    R019 equality for strings is Python's, and a missing right-side value
    matches nothing because absence is not an identity.
    """
    if left is MISSING or right is MISSING:
        return False
    try:
        return compare_values(left, right) == 0
    except TypeError:
        return False


def absent_value(absent: JsonValue) -> RuntimeValue:
    """Return the runtime value an intermediate's decided absence carries."""
    normalized = normalize_runtime_value(absent)
    if isinstance(normalized, ValueResult):
        return normalized.value
    return MISSING


def _names(value: object) -> tuple[str, ...] | None:
    if isinstance(value, str):
        return (value,)
    if isinstance(value, Sequence) and all(isinstance(item, str) for item in value):
        return tuple(value)  # type: ignore[arg-type]
    return None


def _order_terms(
    payload: Mapping[str, object], dataset: str
) -> tuple[tuple[OrderTerm, str], ...] | None:
    """Build the (term, field) pairs an inline `lookup:` orders by."""
    raw = payload.get("order_by")
    if raw is None:
        return None
    if not isinstance(raw, Sequence) or isinstance(raw, str):
        return None
    terms: list[tuple[OrderTerm, str]] = []
    for item in raw:
        if isinstance(item, str):
            term = OrderTerm(variable=item)
        elif isinstance(item, Mapping):
            try:
                term = OrderTerm.model_validate(dict(item))
            except ValidationError:
                return None
        else:
            return None
        qualifier, _, field = term.variable.partition(".")
        if qualifier != dataset:
            return None
        terms.append((term, field))
    return tuple(terms)


def evaluate_intermediate(
    payload: Mapping[str, object],
    relation: RelationIndex,
    resolve: Callable[[str], Resolution],
) -> EvaluationResult:
    """Evaluate one inline `lookup:` operation against its dataset.

    The planner validates the declaration; this answers the row. `resolve`
    reads one current-row variable the way the derivation's own resolver
    does, so a source may name an output column or a driver-qualified
    dataset column exactly as the specification wrote it.
    """
    sources = _names(payload.get("key_base"))
    keys = _names(payload.get("key"))
    value_field = payload.get("value")
    dataset = relation.dataset
    if (
        sources is None
        or keys is None
        or not isinstance(value_field, str)
        or len(sources) != len(keys)
        or not sources
    ):
        return ConditionResult(
            condition=RuntimeCondition(
                phase="validation",
                condition="invalid_field_type",
                context={
                    "operation": "lookup",
                    "expected": "key_base, key, and value",
                },
                requirement="REQ-0321",
            )
        )
    if not relation.has(value_field) or any(not relation.has(key) for key in keys):
        return ConditionResult(
            condition=RuntimeCondition(
                phase="validation",
                condition="unknown_field",
                context={"identifier": f"{dataset}.{value_field}"},
            )
        )

    current: dict[str, RuntimeValue] = {}
    names = list(sources)
    between = payload.get("between")
    between_value: str | None = None
    between_lower: str | None = None
    between_upper: str | None = None
    if between is not None:
        # The planner requires all three together; a payload that reached
        # here without them narrows by a bound it cannot read, so answer the
        # declaration rather than raising on the missing one.
        bounds = (
            [between.get(name) for name in ("value", "lower", "upper")]
            if isinstance(between, Mapping)
            else []
        )
        if len(bounds) != 3 or not all(isinstance(bound, str) for bound in bounds):
            return ConditionResult(
                condition=RuntimeCondition(
                    phase="validation",
                    condition="invalid_field_type",
                    context={
                        "operation": "lookup",
                        "expected": "between value, lower, and upper",
                    },
                    requirement="REQ-0321",
                )
            )
        between_value, between_lower, between_upper = (str(bound) for bound in bounds)
        names.append(between_value)
    for name in names:
        resolution = resolve(name)
        if isinstance(resolution, ResolvedValue):
            current[name] = resolution.value
        elif isinstance(resolution, FailedResolution):
            return ConditionResult(condition=resolution.condition)
        else:
            current[name] = MISSING

    predicate = None
    filter_text = payload.get("filter")
    if isinstance(filter_text, str):
        try:
            predicate = parse_predicate_cached(filter_text)
        except PredicateError:
            return ConditionResult(
                condition=RuntimeCondition(
                    phase="validation",
                    condition="invalid_field_type",
                    context={"identifier": filter_text},
                )
            )
    eligible = eligible_records(relation.records, predicate, relation)
    if isinstance(eligible, ConditionResult):
        return eligible

    terms = _order_terms(payload, dataset)
    keep = payload.get("keep")
    keep_value = keep if keep in ("first", "last") else None

    plan = PlannedIntermediate(
        identifier=f"intermediate({dataset})",
        dataset=dataset,
        path="lookup",
        match_variables=tuple(sources),
        match_fields=tuple(keys),
        filter_predicate=None,
        order_terms=terms or (),
        keep=keep_value,
        between_value=between_value,
        between_lower=between_lower,
        between_upper=between_upper,
        missing=payload.get("missing"),
        strict=bool(payload.get("strict", False)),
        missing_declared="missing" in payload,
    )
    outcome = _select_eligible(plan, eligible, current)
    if outcome.condition is not None:
        return outcome.condition
    if outcome.record is not None:
        resolved = normalize_runtime_value(outcome.record.values[value_field])
        if isinstance(resolved, ValueResult) and outcome.handled_by is not None:
            return resolved.model_copy(update={"handled_by": outcome.handled_by})
        return resolved
    return ValueResult(
        value=absent_value(outcome.absent), handled_by=outcome.handled_by
    )


__all__ = [
    "IntermediateOutcome",
    "IntermediateSelector",
    "absent_value",
    "evaluate_intermediate",
    "types_comparable",
]
