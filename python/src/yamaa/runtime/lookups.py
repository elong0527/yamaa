"""R015 record lookups: select one record once, then read it many times.

A record lookup states its match once and gives the chosen record a name, so
the columns that read it are plainly reading one record. Everything about
reaching that record -- filtering, equality matching, range narrowing, and
ordered selection -- is the join R003 already defines, performed here through
the same helpers so a lookup and an implicit qualified source cannot disagree.
"""

from __future__ import annotations

from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from typing import Literal

from pydantic import JsonValue

from yamaa.models import (
    MISSING,
    ColumnType,
    ConditionResult,
    RuntimeCondition,
    RuntimeValue,
    runtime_type_name,
)
from yamaa.planning import PlannedRecordLookup
from yamaa.runtime.joins import (
    IndexedRecord,
    RelationIndex,
    compare_values,
    eligible_records,
    json_value,
    select_record,
)


@dataclass(frozen=True, slots=True)
class LookupOutcome:
    """What one current row got from a record lookup.

    A selected record, a decided absence, and a failure stay distinct:
    R015-22 keeps a matched record whose value is missing different from a
    match that never happened.
    """

    record: IndexedRecord | None = None
    condition: ConditionResult | None = None
    spec_path: str | None = None


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

    R015-11 lets `int` and `float` compare through R010's promotion and
    requires every other type to match exactly, so no operand is converted
    implicitly to make a range comparison work.
    """
    return left == right or {left, right} <= {"int", "float"}


class RecordLookupSelector:
    """Select at most one record per current row for each declared lookup."""

    def __init__(
        self,
        plans: Sequence[PlannedRecordLookup],
        relations: Mapping[str, RelationIndex],
    ) -> None:
        self.plans = {plan.identifier: plan for plan in plans}
        self._relations = relations
        self._eligible: dict[str, tuple[IndexedRecord, ...] | ConditionResult] = {}

    def declares(self, identifier: str) -> bool:
        return identifier in self.plans

    def _filtered(
        self, plan: PlannedRecordLookup
    ) -> tuple[IndexedRecord, ...] | ConditionResult:
        """Apply the lookup's `filter` once for the whole run.

        R015-4 makes the filter a predicate over the lookup's own dataset, so
        which records are eligible does not vary by current row and the
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
    ) -> LookupOutcome:
        """Choose this row's record, in the order R015-3 lays the steps out."""
        plan = self.plans[identifier]
        eligible = self._filtered(plan)
        if isinstance(eligible, ConditionResult):
            return LookupOutcome(condition=eligible, spec_path=f"{plan.path}.filter")

        incomplete = self._incomplete_match_value(plan, current)
        if incomplete is not None:
            return incomplete

        values = [current.get(name, MISSING) for name in plan.match_variables]
        if any(value is MISSING for value in values):
            # An output key is never missing under R005, so this is only
            # reachable before key verification; nothing can match it.
            return LookupOutcome()
        matched = [
            record
            for record in eligible
            if all(
                _equal(record.values[field], value)
                for field, value in zip(plan.match_fields, values, strict=True)
            )
        ]

        narrowed = self._narrowed(plan, matched, current)
        if isinstance(narrowed, LookupOutcome):
            return narrowed

        if not narrowed:
            return self._unmatched(plan, values)
        if len(narrowed) == 1:
            return LookupOutcome(record=narrowed[0])
        if plan.keep is None:
            # R015-28: more than one surviving record with nothing to choose
            # by is the unhandled multiple match R003 refuses.
            return LookupOutcome(
                condition=_condition(
                    "multiple_matches",
                    "R015-28",
                    {
                        "record_lookup": plan.identifier,
                        "dataset": plan.dataset,
                        "match_count": len(narrowed),
                    },
                ),
                spec_path=plan.path,
            )
        chosen = select_record(narrowed, plan.order_terms, plan.keep)
        if isinstance(chosen, ConditionResult):
            return LookupOutcome(condition=chosen, spec_path=f"{plan.path}.order_by")
        return LookupOutcome(record=chosen)

    def _incomplete_match_value(
        self,
        plan: PlannedRecordLookup,
        current: Mapping[str, RuntimeValue],
    ) -> LookupOutcome | None:
        """Answer a missing declared match value before any record is read.

        R015-16 keeps the two absences disjoint: an incomplete match value is
        answered before a record is looked for, and an unmatched key after.
        """
        if plan.on_output_keys and plan.between_value is None:
            return None
        names = list(plan.match_variables) if not plan.on_output_keys else []
        if plan.between_value is not None:
            names.append(plan.between_value)
        missing = [name for name in names if current.get(name, MISSING) is MISSING]
        if not missing:
            return None
        if plan.incomplete == "missing":
            return LookupOutcome()
        field = "between.value" if missing[0] == plan.between_value else "source"
        return LookupOutcome(
            condition=_condition(
                "incomplete_match_value",
                "R015-17",
                {
                    "record_lookup": plan.identifier,
                    "dataset": plan.dataset,
                    "missing_source": missing[0],
                },
            ),
            spec_path=f"{plan.path}.{field}",
        )

    def _narrowed(
        self,
        plan: PlannedRecordLookup,
        matched: Sequence[IndexedRecord],
        current: Mapping[str, RuntimeValue],
    ) -> list[IndexedRecord] | LookupOutcome:
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
            # R015-10 and R015-12: both endpoints are inclusive, and a
            # record missing a stated bound is ineligible rather than open.
            if lower is MISSING or upper is MISSING:
                continue
            try:
                # R015-10: both endpoints are inclusive.
                if (
                    compare_values(lower, value) <= 0
                    and compare_values(value, upper) <= 0
                ):
                    kept.append(record)
            except TypeError:
                return LookupOutcome(
                    condition=_condition(
                        "incomparable_range_types",
                        "R015-11",
                        {
                            "record_lookup": plan.identifier,
                            "value_type": runtime_type_name(value),
                            "lower_type": runtime_type_name(lower),
                            "upper_type": runtime_type_name(upper),
                        },
                        phase="validation",
                    ),
                    spec_path=f"{plan.path}.between",
                )
        return kept

    def _unmatched(
        self,
        plan: PlannedRecordLookup,
        values: Sequence[RuntimeValue],
    ) -> LookupOutcome:
        if plan.unmatched == "missing":
            return LookupOutcome()
        return LookupOutcome(
            condition=_condition(
                "unmatched_key",
                "R015-18",
                {
                    "record_lookup": plan.identifier,
                    "dataset": plan.dataset,
                    "key": list(plan.match_fields),
                    "lookup_key": {
                        field: json_value(value)
                        for field, value in zip(plan.match_fields, values, strict=True)
                    },
                },
            ),
            spec_path=plan.path,
        )


def _equal(left: RuntimeValue, right: RuntimeValue) -> bool:
    """Compare two match values under the equality their type owns.

    R015-8 uses R019 equality for strings, which is Python's, and a missing
    right-side value matches nothing because absence is not an identity.
    """
    if left is MISSING or right is MISSING:
        return False
    try:
        return compare_values(left, right) == 0
    except TypeError:
        return False


__all__ = [
    "LookupOutcome",
    "PlannedRecordLookup",
    "RecordLookupSelector",
    "types_comparable",
]
