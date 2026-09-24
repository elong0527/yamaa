"""R003 intermediates: select one record once, then read it many times.

An intermediate states its match once and gives the chosen record a name, so the
columns that read it are plainly reading one record. Everything about
reaching that record -- filtering, equality matching, range narrowing, and
ordered selection -- is one explicit declared-key mechanism, so a named
intermediate and an inline `lookup:` cannot disagree.
"""

from __future__ import annotations

import math
from collections.abc import Callable, Mapping, Sequence
from dataclasses import dataclass
from typing import Literal, NamedTuple

from pydantic import JsonValue, ValidationError

from yamaa.expressions.core import (
    AbsentValue,
    CallableResolver,
    FailedResolution,
    MappingResolver,
    Resolution,
    ResolvedValue,
    Resolver,
    normalize_runtime_value,
)
from yamaa.expressions.dispatch import evaluate_expression
from yamaa.expressions.predicates import (
    PredicateError,
    PredicateValue,
    TruthValue,
    evaluate_predicate,
    parse_predicate_cached,
    predicate_identifiers,
)
from yamaa.models import (
    MISSING,
    ColumnType,
    ConditionResult,
    DateTimeValue,
    DateValue,
    EvaluationResult,
    HandlerName,
    RuntimeCondition,
    RuntimeValue,
    ValueResult,
    runtime_type_name,
)
from yamaa.planning import KeyBaseExpression, PlannedIntermediate
from yamaa.runtime.joins import (
    IndexedRecord,
    RelationIndex,
    compare_values,
    eligible_records,
    json_value,
    select_record,
)
from yamaa.specification.models import Expression, OrderTerm
from yamaa.verification.diagnostics import VerificationFailure


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


class _DerivedRecordResolver:
    """Resolve derivation references against one intermediate record.

    REQ-1185 scopes every reference to the intermediate's own dataset: a bare
    name reads the record's stored field, and a qualified name must name the
    dataset. The planner rejects anything else before a record is read, so
    an absent name here is a defect, not a miss.
    """

    def __init__(self, dataset: str, values: Mapping[str, RuntimeValue]) -> None:
        self._dataset = dataset
        self._values = values

    def resolve(self, variable: str) -> Resolution:
        if "." in variable:
            qualifier, _, field = variable.partition(".")
            name = field if qualifier == self._dataset else None
        else:
            name = variable
        if name is None or name not in self._values:
            return AbsentValue(variable=variable)
        return ResolvedValue(value=self._values[name])


class _LookupPredicateResolver(_DerivedRecordResolver):
    """Bind donor-qualified fields and explicitly planned driver references."""

    def __init__(
        self,
        dataset: str,
        values: Mapping[str, RuntimeValue],
        current: Mapping[str, RuntimeValue],
    ) -> None:
        super().__init__(dataset, values)
        self._current = current

    def resolve(self, variable: str) -> Resolution:
        if variable.split(".", 1)[0] == self._dataset or (
            self._dataset == "SELF" and "." not in variable
        ):
            return super().resolve(variable)
        if variable in self._current:
            return ResolvedValue(value=self._current[variable])
        return AbsentValue(variable=variable)


class _DonorPredicateResolver:
    """Resolve a source-only filter against one augmented donor record."""

    def __init__(self, dataset: str, values: Mapping[str, RuntimeValue]) -> None:
        self._dataset = dataset
        self._values = values

    def resolve(self, variable: str) -> Resolution:
        qualifier, separator, field = variable.partition(".")
        if not separator and self._dataset == "SELF":
            field = qualifier
            qualifier = "SELF"
            separator = "."
        if not separator or qualifier != self._dataset or field not in self._values:
            return FailedResolution(
                condition=RuntimeCondition(
                    phase="validation",
                    condition="unknown_field",
                    context={"identifier": variable},
                )
            )
        return ResolvedValue(value=self._values[field])


class _DerivationFailure(NamedTuple):
    """A derivation that raised an expression condition on a record.

    REQ-1185 surfaces the condition at the derivation's own path instead of
    treating the record as a miss.
    """

    name: str
    condition: ConditionResult


def _filter_records(
    records: Sequence[IndexedRecord],
    predicate: dict[str, object] | None,
    dataset: str,
) -> list[IndexedRecord] | ConditionResult:
    """Keep donor records whose source-only predicate answers true."""
    if predicate is None:
        return list(records)
    kept: list[IndexedRecord] = []
    for record in records:
        result = evaluate_predicate(
            predicate, _DonorPredicateResolver(dataset, record.values)
        )
        if isinstance(result, ConditionResult):
            return result
        assert isinstance(result, PredicateValue)
        if result.value is TruthValue.TRUE:
            kept.append(record)
    return kept


class IntermediateSelector:
    """Select at most one record per current row for each declared intermediate."""

    def __init__(
        self,
        plans: Sequence[PlannedIntermediate],
        relations: Mapping[str, RelationIndex],
        evaluate: Callable[[Expression, Resolver], EvaluationResult] | None = None,
    ) -> None:
        self.plans = {plan.identifier: plan for plan in plans}
        self._relations = relations
        self._eligible: dict[
            str, tuple[IndexedRecord, ...] | ConditionResult | _DerivationFailure
        ] = {}
        self._derived: dict[str, tuple[IndexedRecord, ...] | _DerivationFailure] = {}
        self._match_index: dict[
            str, dict[tuple[RuntimeValue, ...], tuple[IndexedRecord, ...]]
        ] = {}
        self._evaluate = evaluate or evaluate_expression
        self._self_records: tuple[IndexedRecord, ...] = ()
        self.uses_self = any(plan.dataset == "SELF" for plan in plans)

    def add_self_records(self, rows: Sequence[Mapping[str, RuntimeValue]]) -> None:
        """Expose completed row templates to subsequent SELF selections."""
        start = len(self._self_records)
        self._self_records += tuple(
            IndexedRecord(position=start + index, values=dict(values))
            for index, values in enumerate(rows)
        )
        for identifier, plan in self.plans.items():
            if plan.dataset == "SELF":
                self._eligible.pop(identifier, None)
                self._derived.pop(identifier, None)
                self._match_index.pop(identifier, None)

    def declares(self, identifier: str) -> bool:
        return identifier in self.plans

    def _filtered(
        self, plan: PlannedIntermediate
    ) -> tuple[IndexedRecord, ...] | ConditionResult | _DerivationFailure:
        """Cache source-only filtering over augmented donor records.

        A target-dependent result must never enter this run-wide cache.
        """
        cached = self._eligible.get(plan.identifier)
        if cached is None:
            records = self._records(plan)
            if isinstance(records, _DerivationFailure):
                self._eligible[plan.identifier] = records
                return records
            predicate = None if plan.filter_variables else plan.filter_predicate
            kept = _filter_records(records, predicate, plan.dataset)
            cached = kept if isinstance(kept, ConditionResult) else tuple(kept)
            self._eligible[plan.identifier] = cached
        return cached

    def _records(
        self, plan: PlannedIntermediate
    ) -> tuple[IndexedRecord, ...] | _DerivationFailure:
        """Return the donor records with derivations computed.

        REQ-1185 computes each derivation once per record and caches the
        augmented records before filtering, matching, and selection.
        """
        source = (
            self._self_records
            if plan.dataset == "SELF"
            else self._relations[plan.dataset].records
        )
        if not plan.derived:
            return tuple(source)
        cached = self._derived.get(plan.identifier)
        if cached is None:
            augmented: list[IndexedRecord] = []
            for record in source:
                outcome = self._augment(plan, record)
                if isinstance(outcome, _DerivationFailure):
                    cached = outcome
                    break
                augmented.append(outcome)
            else:
                cached = tuple(augmented)
            self._derived[plan.identifier] = cached
        return cached

    def _augment(
        self, plan: PlannedIntermediate, record: IndexedRecord
    ) -> IndexedRecord | _DerivationFailure:
        """Compute one record's derivations.

        REQ-1185 surfaces a derivation's expression condition: a record the
        derivation cannot compute is a data error, not a miss. A derivation
        that yields missing leaves the record augmented with missing, which
        simply does not match.
        """
        values = dict(record.values)
        resolver = _DerivedRecordResolver(plan.dataset, values)
        for name, declaration in plan.derived:
            result = self._evaluate(declaration.value, resolver)
            if isinstance(result, ConditionResult):
                return _DerivationFailure(name, result)
            values[name] = result.value
        return IndexedRecord(position=record.position, values=values)

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
        if isinstance(eligible, _DerivationFailure):
            return IntermediateOutcome(
                condition=eligible.condition,
                spec_path=f"{plan.path}.derivations.{eligible.name}",
            )
        if plan.match_expressions:
            # REQ-1259: a key_base expression evaluates against the current
            # row and supplies that key position's match value. A missing
            # result matches nothing, exactly like a missing variable.
            resolved_current = dict(current)
            resolver = MappingResolver(current)
            for keyed in plan.match_expressions:
                result = self._evaluate(keyed.expression, resolver)
                if isinstance(result, ConditionResult):
                    return IntermediateOutcome(
                        condition=result,
                        spec_path=f"{plan.path}.{keyed.name}",
                    )
                resolved_current[keyed.name] = result.value
            current = resolved_current
        return _select_eligible(
            plan, eligible, current, index=self._match_index_for(plan, eligible)
        )

    def verify_uniqueness(
        self, *, self_only: bool = False
    ) -> tuple[VerificationFailure, ...]:
        """Evaluate every declared intermediate uniqueness check (REQ-1245).

        Each check runs over the source-only filtered donor records with
        derivations computed. Input checks precede row construction; SELF
        checks follow each completed template.
        A filter or derivation that failed to materialize fails the run
        here too: the verification is load-bearing for a declared
        intermediate, so its failure cannot wait for a selection that may
        never happen.
        """
        failures: list[VerificationFailure] = []
        for plan in self.plans.values():
            if (plan.dataset == "SELF") != self_only:
                continue
            if not plan.unique_columns:
                continue
            records = self._filtered(plan)
            if isinstance(records, ConditionResult):
                failures.append(
                    _materialization_failure(
                        plan, records.condition, f"{plan.path}.filter"
                    )
                )
                continue
            if isinstance(records, _DerivationFailure):
                failures.append(
                    _materialization_failure(
                        plan,
                        records.condition.condition,
                        f"{plan.path}.derivations.{records.name}",
                    )
                )
                continue
            duplicates = _duplicate_groups(records, plan.unique_columns)
            if duplicates:
                failures.append(
                    VerificationFailure(
                        phase="verification",
                        condition="duplicate_intermediate_records",
                        spec_paths=(f"{plan.path}.verification",),
                        requirement="REQ-1245",
                        context={
                            "intermediate": plan.identifier,
                            "dataset": plan.dataset,
                            "columns": list(plan.unique_columns),
                            "duplicate_count": len(duplicates),
                        },
                        offending_keys=tuple(
                            {
                                field: json_value(record.values[field])
                                for field in plan.unique_columns
                            }
                            for record in duplicates
                        ),
                    )
                )
        return tuple(failures)

    def _match_index_for(
        self,
        plan: PlannedIntermediate,
        eligible: Sequence[IndexedRecord],
    ) -> dict[tuple[RuntimeValue, ...], tuple[IndexedRecord, ...]]:
        """Return the plan's match-key index, building it once per run.

        REQ-0134 matches on equality of every source/key pair; the index
        answers that equality with one hash lookup per row instead of one
        scan of the eligible records per row. Buckets keep eligible order,
        so narrowing, `keep` selection, and multiple-match reporting see
        the same record sequence the scan produced.
        """
        cached = self._match_index.get(plan.identifier)
        if cached is None:
            buckets: dict[tuple[RuntimeValue, ...], list[IndexedRecord]] = {}
            for record in eligible:
                key = _match_key(
                    tuple(record.values[field] for field in plan.match_fields)
                )
                if key is None:
                    continue
                buckets.setdefault(key, []).append(record)
            cached = {key: tuple(records) for key, records in buckets.items()}
            self._match_index[plan.identifier] = cached
        return cached


def _match_key(
    values: Sequence[RuntimeValue],
) -> tuple[RuntimeValue, ...] | None:
    """Return the hash key for one side of a match, or `None` when it cannot match.

    The key preserves `_equal` exactly: missing matches nothing, `bool`
    never compares (R007 orders only str/int/float/date/datetime, and
    `values_comparable` refuses every other pairing), and a non-finite
    float cannot reach the runtime as a value. `int`/`float` share Python
    equality, so `1` and `1.0` key together exactly as `compare_values`
    equates them; `DateValue`/`DateTimeValue` hash by the `ordering_key`
    `compare_values` orders by.
    """
    for value in values:
        if value is MISSING or type(value) is bool:
            return None
        if type(value) is float and not math.isfinite(value):
            return None
        if not isinstance(value, (str, int, float, DateValue, DateTimeValue)):
            return None
    return tuple(values)


def _select_eligible(
    plan: PlannedIntermediate,
    eligible: Sequence[IndexedRecord],
    current: Mapping[str, RuntimeValue],
    *,
    index: Mapping[tuple[RuntimeValue, ...], Sequence[IndexedRecord]] | None = None,
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
    if index is not None:
        key = _match_key(tuple(values))
        matched = list(index.get(key, ())) if key is not None else []
    else:
        matched = [
            record
            for record in eligible
            if all(
                _equal(record.values[field], value)
                for field, value in zip(plan.match_fields, values, strict=True)
            )
        ]

    if plan.filter_variables:
        filtered: list[IndexedRecord] = []
        for record in matched:
            result = evaluate_predicate(
                plan.filter_predicate,
                _LookupPredicateResolver(plan.dataset, record.values, current),
            )
            if isinstance(result, ConditionResult):
                return IntermediateOutcome(
                    condition=result, spec_path=f"{plan.path}.filter"
                )
            assert isinstance(result, PredicateValue)
            if result.value is TruthValue.TRUE:
                filtered.append(record)
        matched = filtered

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


def _uniqueness_key(values: Sequence[RuntimeValue]) -> tuple[object, ...]:
    """Return the grouping key for one record's uniqueness columns.

    Mirrors the output `unique` check's comparable form: missing reads as
    `None`, and dates group by their text form. `int`/`float` share Python
    equality, so `1` and `1.0` group together exactly as matching equates
    them.
    """
    key: list[object] = []
    for value in values:
        if value is MISSING:
            key.append(None)
        elif isinstance(value, (DateValue, DateTimeValue)):
            key.append(value.to_text())
        else:
            key.append(value)
    return tuple(key)


def _materialization_failure(
    plan: PlannedIntermediate,
    failed: RuntimeCondition,
    spec_path: str,
) -> VerificationFailure:
    """Report a verified intermediate whose records never materialized.

    REQ-1245 makes the verification load-bearing: a declared intermediate
    whose filter or derivation failed cannot wait for a selection that may
    never happen, so the original condition surfaces here with the same
    spec path `select` would have reported it at.
    """
    return VerificationFailure(
        phase="verification",
        condition=failed.condition,
        spec_paths=(spec_path,),
        requirement=failed.requirement or "REQ-1245",
        context={"intermediate": plan.identifier, **failed.context},
    )


def _duplicate_groups(
    records: Sequence[IndexedRecord], columns: Sequence[str]
) -> list[IndexedRecord]:
    """Return one representative record per repeated key combination."""
    seen: dict[tuple[object, ...], IndexedRecord] = {}
    duplicates: dict[tuple[object, ...], IndexedRecord] = {}
    for record in records:
        key = _uniqueness_key(tuple(record.values[field] for field in columns))
        if key in seen:
            duplicates.setdefault(key, seen[key])
        else:
            seen[key] = record
    return list(duplicates.values())


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


def _key_base_entries(
    value: object,
) -> tuple[str | Mapping[str, object], ...] | None:
    """Parse an inline lookup's key_base into variables and expressions.

    REQ-1259: an entry is a bare variable or an ordinary expression mapping.
    """
    if isinstance(value, str):
        return (value,)
    if isinstance(value, Mapping) and len(value) == 1:
        return (value,)
    if isinstance(value, Sequence) and not isinstance(value, str):
        entries: list[str | Mapping[str, object]] = []
        for item in value:
            if isinstance(item, str) or isinstance(item, Mapping) and len(item) == 1:
                entries.append(item)
            else:
                return None
        return tuple(entries)
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
    entries = _key_base_entries(payload.get("key_base"))
    keys = _names(payload.get("key"))
    value_field = payload.get("value")
    dataset = relation.dataset
    if (
        entries is None
        or keys is None
        or not isinstance(value_field, str)
        or len(entries) != len(keys)
        or not entries
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
    match_variables: list[str] = []
    match_expressions: list[KeyBaseExpression] = []
    key_resolver = CallableResolver(resolve)
    for index, entry in enumerate(entries):
        if isinstance(entry, str):
            match_variables.append(entry)
            continue
        # REQ-1259: a key_base expression evaluates against the current row
        # through the lookup's own resolver and supplies that key position's
        # match value. A missing result matches nothing.
        expression = Expression.model_validate(dict(entry))
        result = evaluate_expression(expression, key_resolver)
        if isinstance(result, ConditionResult):
            return result
        name = f"key_base[{index}]"
        match_variables.append(name)
        match_expressions.append(
            KeyBaseExpression(name=name, expression=expression, variables=())
        )
        current[name] = result.value
    names = [entry for entry in entries if isinstance(entry, str)]
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
    filter_variables = tuple(
        name
        for name in predicate_identifiers(predicate or {})
        if name.split(".", 1)[0] != dataset
    )
    names.extend(filter_variables)
    for name in names:
        resolution = resolve(name)
        if isinstance(resolution, ResolvedValue):
            current[name] = resolution.value
        elif isinstance(resolution, FailedResolution):
            return ConditionResult(condition=resolution.condition)
        else:
            current[name] = MISSING

    terms = _order_terms(payload, dataset)
    keep = payload.get("keep")
    keep_value = keep if keep in ("first", "last") else None

    plan = PlannedIntermediate(
        identifier=f"intermediate({dataset})",
        dataset=dataset,
        path="lookup",
        match_variables=tuple(match_variables),
        match_expressions=tuple(match_expressions),
        match_fields=tuple(keys),
        filter_predicate=predicate,
        order_terms=terms or (),
        keep=keep_value,
        between_value=between_value,
        between_lower=between_lower,
        between_upper=between_upper,
        missing=payload.get("missing"),
        strict=bool(payload.get("strict", False)),
        missing_declared="missing" in payload,
    )
    eligible = eligible_records(
        relation.records, None if filter_variables else predicate, relation
    )
    if isinstance(eligible, ConditionResult):
        return eligible
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
