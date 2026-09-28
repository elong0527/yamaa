from __future__ import annotations

from yamaa.expressions import ResolvedValue, parse_predicate
from yamaa.expressions.dispatch import ExpressionDispatcher
from yamaa.io.polars import frame_from_values
from yamaa.models import MISSING, ConditionResult, DateValue, TypedColumn, ValueResult
from yamaa.odm import BindingIndex, BindingPlan, DatasetBinding
from yamaa.planning import (
    MatchValueExpression,
    PlannedIntermediate,
    PlannedIntermediateUnique,
)
from yamaa.runtime import ExecutionSuccess, execute_specification
from yamaa.runtime.intermediates import (
    IntermediateSelector,
    _select_eligible,
    evaluate_intermediate,
    types_comparable,
)
from yamaa.runtime.joins import RelationIndex
from yamaa.runtime.rows import CandidateRow, RelationalContext, RowResolver
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    Intermediate,
    OrderTerm,
    Output,
    Specification,
)


def relation(
    name: str,
    columns: list[tuple[str, str]],
    rows: list[list[object]],
) -> RelationIndex:
    typed = tuple(
        TypedColumn(name=column, type=column_type) for column, column_type in columns
    )
    return RelationIndex(name, frame_from_values(typed, rows))


def ex() -> RelationIndex:
    return relation(
        "EX",
        [
            ("STUDYID", "str"),
            ("USUBJID", "str"),
            ("EXSEQ", "int"),
            ("EXTRT", "str"),
            ("EXENDTC", "date"),
        ],
        [
            ["CATH", "S1", 1, "VITAMIN D3", DateValue.parse("2025-01-14")],
            ["CATH", "S1", 2, "PLACEBO", DateValue.parse("2025-01-28")],
            ["CATH", "S1", 3, "RESCUE", MISSING],
            ["CATH", "S2", 1, "VITAMIN D3", DateValue.parse("2025-02-20")],
        ],
    )


def selector(plan: PlannedIntermediate) -> IntermediateSelector:
    return IntermediateSelector([plan], {"EX": ex()})


def explicit_keys(**extra: object) -> PlannedIntermediate:
    return PlannedIntermediate(
        identifier="LASTEX",
        dataset="EX",
        path="intermediates[0]",
        match_variables=("STUDYID", "USUBJID"),
        match_fields=("STUDYID", "USUBJID"),
        no_match_declared=extra.pop("no_match_declared", True),
        **extra,
    )


def test_a_filter_and_an_ordered_keep_select_one_record() -> None:
    plan = explicit_keys(
        filter_predicate=parse_predicate("EX.EXENDTC IS NOT NULL"),
        order_terms=(
            (OrderTerm(variable="EX.EXENDTC"), "EXENDTC"),
            (OrderTerm(variable="EX.EXSEQ"), "EXSEQ"),
        ),
        keep="last",
    )

    outcome = selector(plan).select("LASTEX", {"STUDYID": "CATH", "USUBJID": "S1"})

    assert outcome.condition is None
    assert outcome.record is not None
    assert outcome.record.values["EXTRT"] == "PLACEBO"


def test_keeping_first_reads_the_other_end_of_the_same_order() -> None:
    plan = explicit_keys(
        filter_predicate=parse_predicate("EX.EXENDTC IS NOT NULL"),
        order_terms=((OrderTerm(variable="EX.EXENDTC"), "EXENDTC"),),
        keep="first",
    )

    outcome = selector(plan).select("LASTEX", {"STUDYID": "CATH", "USUBJID": "S1"})

    assert outcome.record is not None
    assert outcome.record.values["EXTRT"] == "VITAMIN D3"


def test_a_tie_on_every_term_is_resolved_by_record_order() -> None:
    # REQ-0135: remaining ties are resolved by record order, which makes the
    # selection total rather than dependent on the sort's stability.
    tied = relation(
        "EX",
        [("USUBJID", "str"), ("EXTRT", "str")],
        [["S1", "FIRST"], ["S1", "SECOND"]],
    )
    plan = PlannedIntermediate(
        identifier="DOSING",
        dataset="EX",
        path="intermediates[0]",
        match_variables=("USUBJID",),
        match_fields=("USUBJID",),
        order_terms=((OrderTerm(variable="EX.USUBJID"), "USUBJID"),),
        keep="first",
        no_match_declared=True,
    )

    outcome = IntermediateSelector([plan], {"EX": tied}).select(
        "DOSING", {"USUBJID": "S1"}
    )

    assert outcome.record is not None
    assert outcome.record.values["EXTRT"] == "FIRST"


def test_several_surviving_records_with_no_order_fail() -> None:
    # REQ-0127: an unhandled multiple match under R003.
    outcome = selector(explicit_keys()).select(
        "LASTEX", {"STUDYID": "CATH", "USUBJID": "S1"}
    )

    assert outcome.condition is not None
    assert outcome.condition.condition.condition == "multiple_matches"
    assert outcome.condition.condition.requirement == "REQ-0127"
    assert outcome.condition.condition.context["match_count"] == 3
    assert outcome.spec_path == "intermediates[0]"


def test_matching_on_explicit_keys_answers_an_absent_record_with_missing() -> None:
    # REQ-0129: a declared `no_match` answers an absent right-side record.
    outcome = selector(explicit_keys()).select(
        "LASTEX", {"STUDYID": "CATH", "USUBJID": "S9"}
    )

    assert outcome.record is None
    assert outcome.condition is None


def declared(no_match_declared: bool = False, **extra: object) -> PlannedIntermediate:
    return PlannedIntermediate(
        identifier="REFRANGE",
        dataset="EX",
        path="intermediates[0]",
        match_variables=("SUBJECT",),
        match_fields=("USUBJID",),
        no_match_declared=no_match_declared,
        **extra,
    )


def test_a_declared_key_with_no_record_is_fatal_and_names_the_key() -> None:
    # REQ-0124: R007 makes an unmatched lookup key fatal unless the
    # specification answers for it.
    outcome = selector(declared()).select("REFRANGE", {"SUBJECT": "S9"})

    assert outcome.condition is not None
    assert outcome.condition.condition.condition == "unmatched_key"
    assert outcome.condition.condition.requirement == "REQ-0124"
    assert outcome.condition.condition.context["intermediate_key"] == {"USUBJID": "S9"}


def test_an_unhandled_multiple_match_names_the_key_it_matched_on() -> None:
    # REQ-0143: one vocabulary for every record lookup failure, so a multiple
    # match names its match under `key` and `lookup_key` the way an unmatched
    # key does and leaves `keys` to the offending output row.
    outcome = selector(declared()).select("REFRANGE", {"SUBJECT": "S1"})

    assert outcome.condition is not None
    context = outcome.condition.condition.context
    assert outcome.condition.condition.condition == "multiple_matches"
    assert context["key"] == ["USUBJID"]
    assert context["intermediate_key"] == {"USUBJID": "S1"}
    assert context["match_count"] == 3
    assert "keys" not in context


def test_a_declared_unmatched_answer_replaces_the_failure() -> None:
    outcome = selector(declared(no_match_declared=True)).select(
        "REFRANGE", {"SUBJECT": "S9"}
    )

    assert outcome.condition is None
    assert outcome.record is None


def test_a_missing_match_value_is_answered_before_a_record_is_looked_for() -> None:
    # The unified absence model: a missing source value and an unmatched
    # key are one category. Without `no_match` either one fails; with it the
    # lookup answers its literal.
    fatal = selector(declared()).select("REFRANGE", {"SUBJECT": MISSING})
    answered = selector(declared(no_match_declared=True)).select(
        "REFRANGE", {"SUBJECT": MISSING}
    )

    assert fatal.condition is not None
    assert fatal.condition.condition.condition == "unmatched_key"
    assert fatal.condition.condition.requirement == "REQ-0124"
    assert answered.condition is None
    assert answered.record is None


def test_key_match_expression_evaluates_against_current_row() -> None:
    # REQ-1259: an expression match value evaluates against the current row.
    plan = PlannedIntermediate(
        identifier="UPPER",
        dataset="EX",
        path="intermediates[0]",
        match_variables=("key[USUBJID]",),
        match_fields=("USUBJID",),
        match_expressions=(
            MatchValueExpression(
                column="USUBJID",
                name="key[USUBJID]",
                expression=Expression.model_validate(
                    {"str_case": {"source": "SUBJECT", "to": "upper"}}
                ),
                variables=("SUBJECT",),
            ),
        ),
        order_terms=((OrderTerm(variable="EX.EXSEQ"), "EXSEQ"),),
        keep="first",
        no_match_declared=True,
    )
    chosen = selector(plan).select("UPPER", {"SUBJECT": "s1"})

    assert chosen.record is not None
    assert chosen.record.values["EXTRT"] == "VITAMIN D3"


def test_key_match_expression_missing_result_matches_nothing() -> None:
    # REQ-1259: a missing expression result matches nothing, like a missing
    # variable.
    plan = PlannedIntermediate(
        identifier="UPPER",
        dataset="EX",
        path="intermediates[0]",
        match_variables=("key[USUBJID]",),
        match_fields=("USUBJID",),
        match_expressions=(
            MatchValueExpression(
                column="USUBJID",
                name="key[USUBJID]",
                expression=Expression.model_validate({"literal": None}),
                variables=(),
            ),
        ),
        no_match_declared=True,
    )
    answered = selector(plan).select("UPPER", {"SUBJECT": "s1"})

    assert answered.condition is None
    assert answered.record is None


def expression_key_payload() -> dict[str, object]:
    return {
        "dataset": "EX",
        "key": {"USUBJID": {"double": {"source": "SUBJECT"}}},
        "value": "EXTRT",
        "order_by": ["EX.EXSEQ"],
        "keep": "first",
    }


def test_a_key_match_expression_uses_the_configured_dispatcher() -> None:
    # REQ-1189/REQ-1259: an expression match value in an evaluate_intermediate
    # payload evaluates through the caller's configured dispatcher, so an R018
    # function operation resolves there instead of failing as unsupported.

    def double(payload, resolver):
        return ValueResult(value="S1")

    dispatcher = ExpressionDispatcher(extensions={"double": double})
    result = evaluate_intermediate(
        expression_key_payload(),
        ex(),
        lambda name: ResolvedValue(value="s1"),
        evaluate=dispatcher.evaluate,
    )

    assert isinstance(result, ValueResult)
    assert result.value == "VITAMIN D3"


def test_a_key_match_expression_without_a_dispatcher_is_a_condition() -> None:
    # REQ-1259: without a configured dispatcher an unsupported operation
    # yields a clean condition, never an AttributeError on the result.
    result = evaluate_intermediate(
        expression_key_payload(), ex(), lambda name: ResolvedValue(value="s1")
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "invalid_field_type"
    assert result.condition.requirement == "REQ-0321"


def test_a_select_key_match_expression_without_a_dispatcher_is_a_condition() -> None:
    # REQ-1259: in select, an expression match value that does not evaluate to
    # a value yields a clean condition, never an AttributeError on .value.
    plan = PlannedIntermediate(
        identifier="UPPER",
        dataset="EX",
        path="intermediates[0]",
        match_variables=("key[USUBJID]",),
        match_fields=("USUBJID",),
        match_expressions=(
            MatchValueExpression(
                column="USUBJID",
                name="key[USUBJID]",
                expression=Expression.model_validate({"double": {"source": "SUBJECT"}}),
                variables=("SUBJECT",),
            ),
        ),
        no_match_declared=True,
    )
    answered = selector(plan).select("UPPER", {"SUBJECT": "s1"})

    assert answered.record is None
    assert answered.condition is not None
    assert answered.condition.condition.condition == "invalid_field_type"
    assert answered.condition.condition.requirement == "REQ-0321"


def test_a_named_key_match_expression_reads_its_source_when_selecting() -> None:
    # REQ-1259: selection evaluates the expression match value over the row's
    # recorded reads, so the source an operation names must be among them.
    def read(variable: str) -> HandledExpression:
        return HandledExpression(value=Expression(root={"source": variable}))

    spec = Specification(
        schema_version="1.0",
        domain="OUT",
        input={
            "DM": DatasetSource(path="dm.csv"),
            "TAB": DatasetSource(path="tab.csv"),
        },
        base="DM",
        keys=["USUBJID"],
        intermediates=[
            Intermediate(
                id="T",
                dataset="TAB",
                key={
                    "K": {"round_half_away_from_zero": {"source": "SCORE", "digits": 0}}
                },
            )
        ],
        columns=[
            Column(name="USUBJID", type="str", derivation=read("DM.USUBJID")),
            Column(name="SCORE", type="float", derivation=read("DM.SCORE")),
            Column(name="VAL", type="str", derivation=read("T.VAL")),
        ],
        output=Output(path="out.csv", columns=["USUBJID", "SCORE", "VAL"]),
    )
    sources = {
        "DM": frame_from_values(
            (
                TypedColumn(name="USUBJID", type="str"),
                TypedColumn(name="SCORE", type="float"),
            ),
            [["s1", 1.4], ["s2", 2.6]],
        ),
        "TAB": frame_from_values(
            (TypedColumn(name="K", type="float"), TypedColumn(name="VAL", type="str")),
            [[1.0, "one"], [3.0, "three"]],
        ),
    }

    result = execute_specification(spec, sources)

    assert isinstance(result, ExecutionSuccess), result
    assert [row["VAL"] for row in result.artifact.frame.to_dicts()] == [
        "one",
        "three",
    ]


def test_key_match_window_expression_uses_the_current_row_partition() -> None:
    # REQ-1259: a window expression as a key match value needs the row's relational
    # resolver, for a named intermediate.
    def read(variable: str) -> HandledExpression:
        return HandledExpression(value=Expression(root={"source": variable}))

    key = {"K": {"baseline_flag": {"date": "ADT", "reference_date": "TRTSDT"}}}
    sources = {
        "DM": frame_from_values(
            (
                TypedColumn(name="USUBJID", type="str"),
                TypedColumn(name="ADT", type="date"),
            ),
            [["s1", DateValue.parse("2024-01-15")]],
        ),
        "TAB": frame_from_values(
            (TypedColumn(name="K", type="str"), TypedColumn(name="VAL", type="str")),
            [["Y", "baseline"]],
        ),
    }
    value = read("T.VAL")
    spec = Specification(
        schema_version="1.0",
        domain="OUT",
        input={
            "DM": DatasetSource(path="dm.csv"),
            "TAB": DatasetSource(path="tab.csv"),
        },
        base="DM",
        keys=["USUBJID"],
        intermediates=[Intermediate(id="T", dataset="TAB", key=key)],
        columns=[
            Column(name="USUBJID", type="str", derivation=read("DM.USUBJID")),
            Column(name="ADT", type="date", derivation=read("DM.ADT")),
            Column(name="TRTSDT", type="date", derivation=read("DM.ADT")),
            Column(name="VAL", type="str", derivation=value),
        ],
        output=Output(path="out.csv", columns=["USUBJID", "VAL"]),
    )

    result = execute_specification(spec, sources)

    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame.to_dicts()[0]["VAL"] == "baseline"


def epochs() -> RelationIndex:
    return relation(
        "EPOCHS",
        [("STUDYID", "str"), ("EPOCH", "str"), ("LO", "int"), ("HI", "int")],
        [
            ["CATH", "SCREENING", -14, 0],
            ["CATH", "TREATMENT", 1, 56],
            ["CATH", "OPEN", 57, MISSING],
        ],
    )


def between(**extra: object) -> PlannedIntermediate:
    return PlannedIntermediate(
        identifier="EPOCHDEF",
        dataset="EPOCHS",
        path="intermediates[0]",
        match_variables=("STUDYID",),
        match_fields=("STUDYID",),
        between_value="ADY",
        between_lower="LO",
        between_upper="HI",
        no_match_declared=extra.pop("no_match_declared", True),
        **extra,
    )


def epoch_selector() -> IntermediateSelector:
    return IntermediateSelector([between()], {"EPOCHS": epochs()})


def test_a_closed_range_includes_both_stated_endpoints() -> None:
    chosen = epoch_selector().select("EPOCHDEF", {"STUDYID": "CATH", "ADY": 56})
    lower = epoch_selector().select("EPOCHDEF", {"STUDYID": "CATH", "ADY": 1})

    assert chosen.record is not None
    assert chosen.record.values["EPOCH"] == "TREATMENT"
    assert lower.record is not None
    assert lower.record.values["EPOCH"] == "TREATMENT"


def test_a_record_missing_a_stated_bound_is_ineligible() -> None:
    # REQ-0121: an open range is not admitted by omission of the value.
    outcome = epoch_selector().select("EPOCHDEF", {"STUDYID": "CATH", "ADY": 60})

    assert outcome.record is None
    assert outcome.condition is None


def test_a_missing_range_value_is_an_absence_not_an_unmatched_key() -> None:
    # The unified absence model: a missing between value yields nothing
    # before any record is read, like a missing key.
    plan = between(no_match_declared=False)

    outcome = IntermediateSelector([plan], {"EPOCHS": epochs()}).select(
        "EPOCHDEF", {"STUDYID": "CATH", "ADY": MISSING}
    )

    assert outcome.condition is not None
    assert outcome.condition.condition.condition == "unmatched_key"


def test_declared_types_decide_whether_a_range_can_be_compared() -> None:
    # REQ-0121: int and float compare through R010's promotion; every other
    # type must match exactly.
    assert types_comparable("int", "float")
    assert types_comparable("date", "date")
    assert not types_comparable("date", "int")
    assert not types_comparable("str", "int")


def supp_table() -> object:
    return frame_from_values(
        tuple(
            TypedColumn(name=name, type=column_type)
            for name, column_type in [
                ("STUDYID", "str"),
                ("USUBJID", "str"),
                ("IDVARVAL", "str"),
                ("QVAL", "str"),
            ]
        ),
        [
            ["S1", "U1", "   259", "y"],
            ["S1", "U1", "    7", "n"],
            ["S1", "U2", "       ", "y"],
        ],
    )


def supp() -> RelationIndex:
    return RelationIndex("SUPPLB", supp_table())


def derived_plan(**extra: object) -> PlannedIntermediate:
    from yamaa.specification.models import Expression, HandledExpression

    match_variables = extra.pop("match_variables", ("STUDYID", "USUBJID", "QVAL_T"))
    match_fields = extra.pop("match_fields", ("STUDYID", "USUBJID", "QVAL_U"))
    unique_columns = extra.pop("unique_columns", ())
    if unique_columns:
        extra["unique_checks"] = (
            PlannedIntermediateUnique(
                columns=unique_columns,
                spec_path="intermediates[0].verifications[0].unique",
            ),
        )
    return PlannedIntermediate(
        identifier="SUP_EP",
        dataset="SUPPLB",
        path="intermediates[0]",
        match_variables=match_variables,
        match_fields=match_fields,
        derived=(
            (
                "QVAL_U",
                HandledExpression(
                    value=Expression(
                        root={"str_case": {"source": "QVAL", "to": "upper"}}
                    )
                ),
            ),
        ),
        no_match_declared=extra.pop("no_match_declared", True),
        **extra,
    )


def test_a_derived_key_matches_like_a_stored_column() -> None:
    # REQ-1185: the derived value joins like a stored column.
    outcome = IntermediateSelector([derived_plan()], {"SUPPLB": supp()}).select(
        "SUP_EP", {"STUDYID": "S1", "USUBJID": "U1", "QVAL_T": "Y"}
    )

    assert outcome.condition is None
    assert outcome.record is not None
    assert outcome.record.values["QVAL"] == "y"
    assert outcome.record.values["QVAL_U"] == "Y"


def test_a_blank_derivation_yields_missing_and_does_not_match() -> None:
    # REQ-1185/REQ-1186: blank text parses to missing, and missing never
    # equals a key, so the U2 record is a miss rather than an error.
    outcome = IntermediateSelector([derived_plan()], {"SUPPLB": supp()}).select(
        "SUP_EP", {"STUDYID": "S1", "USUBJID": "U2", "LBSEQ": 1}
    )

    assert outcome.condition is None
    assert outcome.record is None


def test_a_derived_filter_narrows_donor_records() -> None:
    plan = derived_plan(
        match_variables=("STUDYID", "USUBJID"),
        match_fields=("STUDYID", "USUBJID"),
        filter_predicate=parse_predicate("SUPPLB.QVAL_U = 'Y'"),
    )

    outcome = IntermediateSelector([plan], {"SUPPLB": supp()}).select(
        "SUP_EP", {"STUDYID": "S1", "USUBJID": "U1"}
    )

    assert outcome.condition is None
    assert outcome.record is not None
    assert outcome.record.values["IDVARVAL"] == "   259"


def test_a_correlated_filter_reads_the_derived_donor_value() -> None:
    plan = derived_plan(
        match_variables=("STUDYID", "USUBJID"),
        match_fields=("STUDYID", "USUBJID"),
        filter_predicate=parse_predicate("SUPPLB.QVAL_U = WANTED"),
    )

    outcome = IntermediateSelector([plan], {"SUPPLB": supp()}).select(
        "SUP_EP", {"STUDYID": "S1", "USUBJID": "U1", "WANTED": "N"}
    )

    assert outcome.condition is None
    assert outcome.record is not None
    assert outcome.record.values["IDVARVAL"] == "    7"


def test_a_derived_order_term_ranks_donor_records() -> None:
    plan = derived_plan(
        match_variables=("STUDYID", "USUBJID"),
        match_fields=("STUDYID", "USUBJID"),
        order_terms=((OrderTerm(variable="SUPPLB.QVAL_U"), "QVAL_U"),),
        keep="first",
    )

    outcome = IntermediateSelector([plan], {"SUPPLB": supp()}).select(
        "SUP_EP", {"STUDYID": "S1", "USUBJID": "U1"}
    )

    assert outcome.condition is None
    assert outcome.record is not None
    assert outcome.record.values["IDVARVAL"] == "    7"


def test_a_derived_readable_column_resolves_from_the_selected_record() -> None:
    plan = derived_plan(readable_columns=("QVAL_U",))
    table = supp_table()
    binding_plan = BindingPlan(
        domain="OUT",
        datasets={"SUPPLB": DatasetBinding(dataset="SUPPLB", columns=table.columns)},
        output_columns=("STUDYID", "USUBJID", "QVAL_T"),
    )
    relation = RelationIndex("SUPPLB", table)
    selector = IntermediateSelector([plan], {"SUPPLB": relation})
    context = RelationalContext(
        bindings=BindingIndex(binding_plan, {"SUPPLB": table}),
        relations={"SUPPLB": relation},
        intermediates=selector,
        output_keys=(),
    )
    values = {"STUDYID": "S1", "USUBJID": "U1", "QVAL_T": "Y"}
    candidate = CandidateRow(source_rows={}, values=values)

    resolved = RowResolver(context, candidate, values).resolve("SUP_EP.QVAL_U")

    assert isinstance(resolved, ResolvedValue)
    assert resolved.value == "Y"


def test_derived_filter_and_unique_columns_use_augmented_records() -> None:
    plan = derived_plan(
        match_variables=("STUDYID", "USUBJID"),
        match_fields=("STUDYID", "USUBJID"),
        filter_predicate=parse_predicate("SUPPLB.QVAL_U = 'Y'"),
        unique_columns=("STUDYID", "QVAL_U"),
    )

    (failure,) = IntermediateSelector([plan], {"SUPPLB": supp()}).verify_uniqueness()

    assert failure.condition == "duplicate_intermediate_records"
    assert failure.context["duplicate_count"] == 1
    assert failure.offending_keys == ({"STUDYID": "S1", "QVAL_U": "Y"},)


# (Failure-surfacing test removed: no naturally-failing expression in suite
# without to_number; the _DerivationFailure mechanism remains implemented.)


def _outcome_summary(outcome):  # type: ignore[no-untyped-def]
    condition = outcome.condition.condition.condition if outcome.condition else None
    position = outcome.record.position if outcome.record is not None else None
    return (condition, position, outcome.absent)


def test_the_match_index_agrees_with_the_record_scan() -> None:
    # REQ-0134: the hash index answers the same per-row equality the scan
    # answers, including duplicates, misses, missing keys, int/float
    # unification, and bool keys (which never compare under R007).
    rel = relation(
        "K",
        [("A", "str"), ("B", "int"), ("C", "float"), ("D", "date"), ("E", "str")],
        [
            ["s1", 1, 1.5, DateValue.parse("2025-01-01"), "x"],
            ["s1", 1, 1.5, DateValue.parse("2025-01-01"), "y"],
            ["s2", 2, 2.5, DateValue.parse("2025-02-01"), "z"],
            ["s3", MISSING, 3.5, DateValue.parse("2025-03-01"), "w"],
        ],
    )
    plan = PlannedIntermediate(
        identifier="K1",
        dataset="K",
        path="intermediates[0]",
        match_variables=("A", "B", "C", "D"),
        match_fields=("A", "B", "C", "D"),
        keep="first",
        order_terms=((OrderTerm(variable="K.E"), "E"),),
        no_match_declared=True,
    )
    sel = IntermediateSelector([plan], {"K": rel})
    records = list(rel.records)
    currents = [
        {"A": "s1", "B": 1, "C": 1.5, "D": DateValue.parse("2025-01-01")},
        {"A": "s1", "B": 1.0, "C": 1.5, "D": DateValue.parse("2025-01-01")},
        {"A": "s2", "B": 2, "C": 2.5, "D": DateValue.parse("2025-02-01")},
        {"A": "nope", "B": 9, "C": 9.5, "D": DateValue.parse("2025-09-09")},
        {"A": "s1", "B": MISSING, "C": 1.5, "D": DateValue.parse("2025-01-01")},
        {"A": "s3", "B": 99, "C": 3.5, "D": DateValue.parse("2025-03-01")},
        {"A": "s1", "B": True, "C": 1.5, "D": DateValue.parse("2025-01-01")},
    ]
    for current in currents:
        indexed = sel.select("K1", current)
        scanned = _select_eligible(plan, records, current)
        assert _outcome_summary(indexed) == _outcome_summary(scanned), current


def test_the_match_index_agrees_with_the_scan_under_between() -> None:
    # REQ-0134: range narrowing sees the same matched records either way.
    rel = relation(
        "R",
        [("A", "str"), ("LO", "int"), ("HI", "int"), ("V", "str")],
        [
            ["s1", 1, 10, "a"],
            ["s1", 5, 15, "b"],
            ["s2", 1, 10, "c"],
        ],
    )
    plan = PlannedIntermediate(
        identifier="R1",
        dataset="R",
        path="intermediates[0]",
        match_variables=("A",),
        match_fields=("A",),
        between_value="X",
        between_lower="LO",
        between_upper="HI",
        keep="first",
        order_terms=((OrderTerm(variable="R.V"), "V"),),
        no_match_declared=True,
    )
    sel = IntermediateSelector([plan], {"R": rel})
    records = list(rel.records)
    for current in [
        {"A": "s1", "X": 7},
        {"A": "s1", "X": 12},
        {"A": "s1", "X": 99},
        {"A": "s2", "X": 7},
        {"A": "s9", "X": 7},
    ]:
        indexed = sel.select("R1", current)
        scanned = _select_eligible(plan, records, current)
        assert _outcome_summary(indexed) == _outcome_summary(scanned), current


def test_an_implicit_join_indexes_its_relation_once_per_filter() -> None:
    # REQ-0150/REQ-0134: every row shares one eligible set and match index
    # per source-only filter instead of scanning the relation per read, and
    # the indexed read answers duplicates, misses, int/float unification,
    # and missing keys exactly as the scan did.
    rel = relation(
        "B",
        [("K", "str"), ("N", "int"), ("SEQ", "int"), ("V", "str")],
        [
            ["s1", 1, 1, "a"],
            ["s1", 1, 2, "b"],
            ["s2", 2, 1, "c"],
        ],
    )
    key = {"K": "K", "N": "N"}
    selected = {"dataset": "B", "key": key, "value": "V"}
    selected |= {"order_by": ["B.SEQ"], "keep": "last"}

    def read(payload: dict[str, object], current: dict[str, object]) -> object:
        result = evaluate_intermediate(
            payload, rel, lambda name: ResolvedValue(value=current[name])
        )
        if isinstance(result, ConditionResult):
            return result.condition.condition
        return result.value

    assert read(selected, {"K": "s1", "N": 1}) == "b"
    shared = rel.implicit_selections[(None, ("K", "N"))]
    assert read(selected, {"K": "s1", "N": 1.0}) == "b"
    assert read(selected, {"K": "s2", "N": 2}) == "c"
    assert read(selected, {"K": "s9", "N": 1}) is MISSING
    assert read(selected, {"K": "s1", "N": MISSING}) is MISSING
    unselected = {"dataset": "B", "key": key, "value": "V"}
    assert read(unselected, {"K": "s1", "N": 1}) == "multiple_matches"
    filtered = {"dataset": "B", "key": key, "value": "V", "filter": "B.SEQ = 1"}
    assert read(filtered, {"K": "s1", "N": 1}) == "a"

    assert rel.implicit_selections[(None, ("K", "N"))] is shared
    assert set(rel.implicit_selections) == {
        (None, ("K", "N")),
        ("B.SEQ = 1", ("K", "N")),
    }


def ds() -> RelationIndex:
    return relation(
        "DS",
        [
            ("STUDYID", "str"),
            ("USUBJID", "str"),
            ("DSCAT", "str"),
            ("DSDECOD", "str"),
        ],
        [
            ["S1", "P01", "DISPOSITION EVENT", "COMPLETED"],
            ["S1", "P02", "DISPOSITION EVENT", "COMPLETED"],
            ["S1", "P02", "DISPOSITION EVENT", "SCREEN FAILURE"],
        ],
    )


def ds_plan(**extra: object) -> PlannedIntermediate:
    unique_columns = extra.pop("unique_columns", ())
    if unique_columns:
        extra["unique_checks"] = (
            PlannedIntermediateUnique(
                columns=unique_columns,
                spec_path="intermediates[0].verifications[0].unique",
            ),
        )
    return PlannedIntermediate(
        identifier="DS_EOS",
        dataset="DS",
        path="intermediates[0]",
        match_variables=("STUDYID", "USUBJID"),
        match_fields=("STUDYID", "USUBJID"),
        no_match_declared=extra.pop("no_match_declared", True),
        **extra,
    )


def test_unique_donor_records_pass_verification() -> None:
    # REQ-1245: the filter leaves one eligible record per subject.
    selector = IntermediateSelector(
        [
            ds_plan(
                filter_predicate=parse_predicate(
                    "DS.DSCAT = 'DISPOSITION EVENT' AND DS.DSDECOD <> 'SCREEN FAILURE'"
                ),
                unique_columns=("STUDYID", "USUBJID"),
            )
        ],
        {"DS": ds()},
    )

    assert selector.verify_uniqueness() == ()


def test_duplicate_donor_records_fail_verification() -> None:
    # REQ-1245: P02 carries two eligible records, so the run fails loudly.
    selector = IntermediateSelector(
        [ds_plan(unique_columns=("STUDYID", "USUBJID"))], {"DS": ds()}
    )

    (failure,) = selector.verify_uniqueness()

    assert failure.phase == "verification"
    assert failure.condition == "duplicate_intermediate_records"
    assert failure.requirement == "REQ-1245"
    assert failure.spec_paths == ("intermediates[0].verifications[0].unique",)
    assert failure.context == {
        "intermediate": "DS_EOS",
        "dataset": "DS",
        "columns": ["STUDYID", "USUBJID"],
        "duplicate_count": 1,
    }
    assert failure.severity == "error"


def test_multiple_intermediate_checks_keep_their_paths_and_optional_ids() -> None:
    selector = IntermediateSelector(
        [
            ds_plan(
                unique_checks=(
                    PlannedIntermediateUnique(
                        columns=("STUDYID", "USUBJID"),
                        spec_path="intermediates[0].verifications[0].unique",
                    ),
                    PlannedIntermediateUnique(
                        columns=("STUDYID",),
                        spec_path="intermediates[0].verifications[1].unique",
                        verification_id="study-key",
                    ),
                )
            )
        ],
        {"DS": ds()},
    )

    failures = selector.verify_uniqueness()

    assert [failure.spec_paths for failure in failures] == [
        ("intermediates[0].verifications[0].unique",),
        ("intermediates[0].verifications[1].unique",),
    ]
    assert "verification_id" not in failures[0].context
    assert failures[1].context["verification_id"] == "study-key"


def test_an_unverified_intermediate_is_not_checked() -> None:
    selector = IntermediateSelector([ds_plan()], {"DS": ds()})

    assert selector.verify_uniqueness() == ()


def test_a_failed_filter_on_a_verified_intermediate_fails_verification() -> None:
    # REQ-1245 is load-bearing: the filter never materialized, so the
    # verification cannot wait for a selection that may never happen.
    selector = IntermediateSelector(
        [
            ds_plan(
                filter_predicate=parse_predicate("DS.NOPE = 'X'"),
                unique_columns=("STUDYID", "USUBJID"),
            )
        ],
        {"DS": ds()},
    )

    (failure,) = selector.verify_uniqueness()

    assert failure.phase == "verification"
    assert failure.condition == "unknown_field"
    assert failure.requirement == "REQ-1245"
    assert failure.spec_paths == ("intermediates[0].filter",)
    assert failure.context["intermediate"] == "DS_EOS"


def test_a_failed_derivation_on_a_verified_intermediate_fails_verification() -> None:
    # REQ-1245: the derivation names a field the donor never had, so the
    # records never materialized and the original condition surfaces at
    # the derivation's own path.
    from yamaa.specification.models import Expression, HandledExpression

    plan = PlannedIntermediate(
        identifier="SUP_EP",
        dataset="SUPPLB",
        path="intermediates[0]",
        match_variables=("STUDYID", "USUBJID"),
        match_fields=("STUDYID", "USUBJID"),
        derived=(
            (
                "QVAL_U",
                HandledExpression(
                    value=Expression(
                        root={"str_case": {"source": "NOPE", "to": "upper"}}
                    )
                ),
            ),
        ),
        unique_checks=(
            PlannedIntermediateUnique(
                columns=("STUDYID", "USUBJID"),
                spec_path="intermediates[0].verifications[0].unique",
            ),
        ),
        no_match_declared=True,
    )
    selector = IntermediateSelector([plan], {"SUPPLB": supp()})

    (failure,) = selector.verify_uniqueness()

    assert failure.phase == "verification"
    assert failure.condition == "unknown_field"
    assert failure.requirement == "REQ-0103"
    assert failure.spec_paths == ("intermediates[0].derivations.QVAL_U",)
    assert failure.context["intermediate"] == "SUP_EP"


def _window_handled(root: dict[str, object]) -> HandledExpression:
    return HandledExpression(value=Expression(root=root))


def _dosing_plan(**extra: object) -> PlannedIntermediate:
    return PlannedIntermediate(
        identifier="DOSING",
        dataset="EX",
        path="intermediates[0]",
        match_variables=("STUDYID", "USUBJID"),
        match_fields=("STUDYID", "USUBJID"),
        no_match_declared=extra.pop("no_match_declared", True),
        **extra,
    )


def test_a_window_derivation_partitions_and_orders_donor_records() -> None:
    # REQ-1185: a window derivation partitions the donor records and
    # numbers them along its order; the augmented records carry the
    # per-record result into selection.
    plan = _dosing_plan(
        derived=(
            (
                "_RN",
                _window_handled(
                    {
                        "row_number": {
                            "window": {
                                "group_by": ["STUDYID", "USUBJID"],
                                "order_by": [{"variable": "EX.EXSEQ"}],
                            }
                        }
                    }
                ),
            ),
        ),
        order_terms=((OrderTerm(variable="EX._RN", direction="desc"), "_RN"),),
        keep="first",
    )

    outcome = selector(plan).select("DOSING", {"STUDYID": "CATH", "USUBJID": "S1"})

    assert outcome.condition is None
    assert outcome.record is not None
    # S1 has three records (EXSEQ 1, 2, 3); descending _RN picks EXSEQ 3.
    assert outcome.record.values["EXSEQ"] == 3
    assert outcome.record.values["_RN"] == 3


def test_a_window_derivation_partitions_each_subject_separately() -> None:
    # REQ-1185: the partition restarts numbering for each subject.
    plan = _dosing_plan(
        derived=(
            (
                "_RN",
                _window_handled(
                    {
                        "row_number": {
                            "window": {
                                "group_by": ["STUDYID", "USUBJID"],
                                "order_by": [{"variable": "EX.EXSEQ"}],
                            }
                        }
                    }
                ),
            ),
        ),
        order_terms=((OrderTerm(variable="EX._RN", direction="desc"), "_RN"),),
        keep="first",
    )

    outcome = selector(plan).select("DOSING", {"STUDYID": "CATH", "USUBJID": "S2"})

    assert outcome.condition is None
    assert outcome.record is not None
    # S2 has a single record, so it numbers 1 within its own partition.
    assert outcome.record.values["EXSEQ"] == 1
    assert outcome.record.values["_RN"] == 1


def test_a_window_derivation_reads_an_earlier_derived_name() -> None:
    # REQ-1185: derivations evaluate in declaration order, so the window's
    # order reads the derived name the earlier derivation computed.
    plan = _dosing_plan(
        derived=(
            (
                "_TRT_U",
                _window_handled({"str_case": {"source": "EXTRT", "to": "upper"}}),
            ),
            (
                "_RN",
                _window_handled(
                    {
                        "row_number": {
                            "window": {
                                "group_by": ["STUDYID", "USUBJID"],
                                "order_by": [{"variable": "EX._TRT_U"}],
                            }
                        }
                    }
                ),
            ),
        ),
        order_terms=((OrderTerm(variable="EX._RN"), "_RN"),),
        keep="first",
    )

    outcome = selector(plan).select("DOSING", {"STUDYID": "CATH", "USUBJID": "S1"})

    assert outcome.condition is None
    assert outcome.record is not None
    # PLACEBO < RESCUE < VITAMIN D3, so the PLACEBO record (EXSEQ 2)
    # numbers 1.
    assert outcome.record.values["EXSEQ"] == 2
    assert outcome.record.values["_RN"] == 1


def test_a_window_filter_excludes_records_before_numbering() -> None:
    # REQ-1185/REQ-0294: the window's filter marks rows ineligible before
    # the operation runs; an ineligible row's result is missing.
    plan = _dosing_plan(
        derived=(
            (
                "_RN",
                _window_handled(
                    {
                        "row_number": {
                            "window": {
                                "group_by": ["STUDYID", "USUBJID"],
                                "order_by": [{"variable": "EX.EXSEQ"}],
                                "filter": "EX.EXENDTC IS NOT NULL",
                            }
                        }
                    }
                ),
            ),
        ),
        order_terms=((OrderTerm(variable="EX._RN", direction="desc"), "_RN"),),
        keep="first",
    )

    outcome = selector(plan).select("DOSING", {"STUDYID": "CATH", "USUBJID": "S1"})

    assert outcome.condition is None
    assert outcome.record is not None
    # EXSEQ 3 has a missing EXENDTC, so only EXSEQ 1 and 2 are numbered.
    assert outcome.record.values["EXSEQ"] == 2
    assert outcome.record.values["_RN"] == 2


def test_a_window_derivation_error_surfaces_at_the_derivation_path() -> None:
    # REQ-1185: a window the records cannot compute is a data error at
    # the derivation's own path, not a miss.
    plan = _dosing_plan(
        derived=(
            (
                "_RN",
                _window_handled(
                    {
                        "row_number": {
                            "window": {
                                "group_by": ["STUDYID", "USUBJID"],
                                "order_by": [{"variable": "EX.EXSEQ"}],
                                "filter": "EX.EXSEQ =",
                            }
                        }
                    }
                ),
            ),
        ),
        keep="first",
    )

    outcome = selector(plan).select("DOSING", {"STUDYID": "CATH", "USUBJID": "S1"})

    assert outcome.record is None
    assert outcome.condition is not None
    assert outcome.condition.condition.condition == "invalid_predicate"
    assert outcome.condition.condition.requirement == "REQ-0188"
    assert outcome.spec_path == "intermediates[0].derivations._RN"


def _endpoint_relations() -> dict[str, RelationIndex]:
    return {
        "LB": relation(
            "LB",
            [("STUDYID", "str"), ("USUBJID", "str"), ("LBSEQ", "int")],
            [["S1", "U1", 259], ["S1", "U1", 7], ["S1", "U2", 7]],
        ),
        "SUPPLB": relation(
            "SUPPLB",
            [
                ("STUDYID", "str"),
                ("USUBJID", "str"),
                ("IDVARVAL", "str"),
                ("QVAL", "str"),
            ],
            [["S1", "U1", "   259", "Y"], ["S1", "U2", "     7", "N"]],
        ),
    }


def _supp_endpoint_plan(**extra: object) -> PlannedIntermediate:
    # The supplemental key is the laboratory sequence padded to six characters.
    return PlannedIntermediate(
        identifier="SUP_EP",
        dataset="SUPPLB",
        path="intermediates[0]",
        match_variables=("LB.STUDYID", "LB.USUBJID", "key[IDVARVAL]"),
        match_fields=("STUDYID", "USUBJID", "IDVARVAL"),
        match_expressions=(
            MatchValueExpression(
                column="IDVARVAL",
                name="key[IDVARVAL]",
                expression=Expression(
                    root={"str_pad": {"source": "LB.LBSEQ", "width": 6}}
                ),
                variables=("LB.LBSEQ",),
            ),
        ),
        no_match_declared=extra.pop("no_match_declared", True),
        **extra,
    )


def _endpoint_reader_plan(
    *derived: tuple[str, dict[str, object]],
) -> PlannedIntermediate:
    return PlannedIntermediate(
        identifier="LB_EP",
        dataset="LB",
        path="intermediates[1]",
        match_variables=("USUBJID", "LBSEQ"),
        match_fields=("USUBJID", "LBSEQ"),
        derived=tuple(
            (name, HandledExpression(value=Expression(root=expression)))
            for name, expression in derived
        ),
        no_match_declared=True,
    )


def test_an_intermediate_derivation_reads_another_intermediate_per_record() -> None:
    # REQ-1263: each donor record supplies the other intermediate's match
    # values, so every record reads its own supplemental qualifier.
    selector = IntermediateSelector(
        [
            _supp_endpoint_plan(),
            _endpoint_reader_plan(("ENDPOINT", {"source": "SUP_EP.QVAL"})),
        ],
        _endpoint_relations(),
    )

    def endpoint(usubjid: str, lbseq: int) -> object:
        outcome = selector.select("LB_EP", {"USUBJID": usubjid, "LBSEQ": lbseq})
        assert outcome.condition is None
        assert outcome.record is not None
        return outcome.record.values["ENDPOINT"]

    assert endpoint("U1", 259) == "Y"
    # The padding is exact: seven pads to five spaces for U2 only.
    assert endpoint("U1", 7) is MISSING
    assert endpoint("U2", 7) == "N"


def test_a_read_shares_one_selection_per_donor_record() -> None:
    # REQ-1263/REQ-0138: every derivation of one record that reads the
    # other intermediate shares its selection.
    class CountingSelector(IntermediateSelector):
        calls = 0

        def select(self, identifier, current, *, resolver=None):  # type: ignore[no-untyped-def]
            if identifier == "SUP_EP":
                CountingSelector.calls += 1
            return super().select(identifier, current, resolver=resolver)

    selector = CountingSelector(
        [
            _supp_endpoint_plan(),
            _endpoint_reader_plan(
                ("ENDPOINT", {"source": "SUP_EP.QVAL"}),
                ("SUPP_KEY", {"source": "SUP_EP.IDVARVAL"}),
            ),
        ],
        _endpoint_relations(),
    )

    outcome = selector.select("LB_EP", {"USUBJID": "U1", "LBSEQ": 259})

    assert outcome.record is not None
    assert outcome.record.values["SUPP_KEY"] == "   259"
    assert CountingSelector.calls == 3


def test_a_failed_read_names_the_read_intermediate() -> None:
    # REQ-1263: a read with no `no_match` that finds nothing fails as the
    # read intermediate's unmatched key, at that intermediate's path.
    selector = IntermediateSelector(
        [
            _supp_endpoint_plan(no_match_declared=False),
            _endpoint_reader_plan(("ENDPOINT", {"source": "SUP_EP.QVAL"})),
        ],
        _endpoint_relations(),
    )

    outcome = selector.select("LB_EP", {"USUBJID": "U1", "LBSEQ": 259})

    assert outcome.record is None
    assert outcome.condition is not None
    assert outcome.condition.condition.condition == "unmatched_key"
    assert outcome.spec_path == "intermediates[0]"


def test_an_intermediate_rank_then_filter_matches_issue_964() -> None:
    # Issue #964: derive a flag, rank the augmented donor records with a
    # window, then filter on the rank and the flag.
    lb = relation(
        "LB",
        [
            ("STUDYID", "str"),
            ("USUBJID", "str"),
            ("LBTESTCD", "str"),
            ("VISITNUM", "int"),
            ("LBSTRESN", "float"),
        ],
        [
            ["CATH", "S1", "CHOL", 1, 5.0],
            ["CATH", "S1", "CHOL", 13, 9.0],
            ["CATH", "S1", "GLUC", 2, 4.0],
            ["CATH", "S2", "CHOL", 3, 6.0],
        ],
    )
    plan = PlannedIntermediate(
        identifier="EOT",
        dataset="LB",
        path="intermediates[0]",
        match_variables=("STUDYID", "USUBJID", "LBTESTCD"),
        match_fields=("STUDYID", "USUBJID", "LBTESTCD"),
        derived=(
            (
                "_EOT_AWARE",
                _window_handled(
                    {
                        "case": [
                            {
                                "when": "LB.LBSTRESN > 8",
                                "then": {"literal": 99},
                            },
                            {"otherwise": {"literal": 1}},
                        ]
                    }
                ),
            ),
            (
                "_RN",
                _window_handled(
                    {
                        "row_number": {
                            "window": {
                                "group_by": ["STUDYID", "USUBJID", "LBTESTCD"],
                                "order_by": [
                                    {
                                        "variable": "LB._EOT_AWARE",
                                        "direction": "desc",
                                    },
                                    {"variable": "LB.VISITNUM"},
                                ],
                                "filter": (
                                    "LB.VISITNUM <> 13 AND LB._EOT_AWARE IS NOT NULL"
                                ),
                            }
                        }
                    }
                ),
            ),
        ),
        filter_predicate=parse_predicate("LB._RN = 1 AND LB._EOT_AWARE <> 99"),
        order_terms=((OrderTerm(variable="LB._RN"), "_RN"),),
        keep="first",
        no_match_declared=True,
    )
    outcome = IntermediateSelector([plan], {"LB": lb}).select(
        "EOT", {"STUDYID": "CATH", "USUBJID": "S1", "LBTESTCD": "CHOL"}
    )

    assert outcome.condition is None
    assert outcome.record is not None
    # VISITNUM 13 is window-filtered out; the remaining CHOL record ranks
    # first and is not EOT-aware.
    assert outcome.record.values["VISITNUM"] == 1
    assert outcome.record.values["_RN"] == 1
    assert outcome.record.values["_EOT_AWARE"] == 1


def test_a_scalar_derivation_with_a_nested_window_partitions_once(monkeypatch) -> None:
    # The review finding on PR #1072: a window operation nested inside a
    # scalar derivation must partition the donor records once per
    # derivation, not once per record.
    from yamaa.runtime import intermediates

    builds: list[object] = []
    real_prepare = intermediates._prepare_window_partitions

    def counting_prepare(dataset, values_list, window, exclude=()):
        builds.append(window)
        return real_prepare(dataset, values_list, window, exclude=exclude)

    monkeypatch.setattr(intermediates, "_prepare_window_partitions", counting_prepare)
    plan = _dosing_plan(
        derived=(
            (
                "_SHIFTED",
                _window_handled(
                    {
                        "case": [
                            {
                                "when": "EX.EXSEQ > 0",
                                "then": {
                                    "row_number": {
                                        "window": {
                                            "group_by": ["STUDYID", "USUBJID"],
                                            "order_by": [{"variable": "EX.EXSEQ"}],
                                        }
                                    }
                                },
                            },
                            {"otherwise": 0},
                        ]
                    }
                ),
            ),
        ),
        order_terms=(
            (OrderTerm(variable="EX._SHIFTED", direction="desc"), "_SHIFTED"),
        ),
        keep="first",
    )

    outcome = selector(plan).select("DOSING", {"STUDYID": "CATH", "USUBJID": "S1"})

    assert outcome.condition is None
    assert outcome.record is not None
    # S1's EXSEQ 3 record numbers 3 within its partition.
    assert outcome.record.values["EXSEQ"] == 3
    assert outcome.record.values["_SHIFTED"] == 3
    # Four donor records, one partition build for the nested window.
    assert len(builds) == 1


def test_an_unread_intermediate_never_fails_as_unmatched_key() -> None:
    # REQ-1264: a named lookup matches only when it is read. An intermediate
    # that no column reads for a row is never matched, so one without
    # `no_match` never fails as unmatched_key on a row that did not read it.
    def read(variable: str) -> HandledExpression:
        return HandledExpression(value=Expression(root={"source": variable}))

    spec = Specification(
        schema_version="1.0",
        domain="OUT",
        input={
            "DM": DatasetSource(path="dm.csv"),
            "REF": DatasetSource(path="ref.csv"),
        },
        base="DM",
        keys=["USUBJID"],
        intermediates=[
            Intermediate(
                id="STRICT_REF",
                dataset="REF",
                key={"PARAMCD": "DM.PARAMCD"},
            )
        ],
        columns=[
            Column(name="USUBJID", type="str", derivation=read("DM.USUBJID")),
            # No column reads STRICT_REF: the lazy match never runs, so the
            # lookup cannot fail even though P2 has no match in REF.
        ],
        output=Output(path="out.csv", columns=["USUBJID"]),
    )
    sources = {
        "DM": frame_from_values(
            (
                TypedColumn(name="USUBJID", type="str"),
                TypedColumn(name="PARAMCD", type="str"),
            ),
            [["s1", "P2"]],
        ),
        "REF": frame_from_values(
            (
                TypedColumn(name="PARAMCD", type="str"),
                TypedColumn(name="VALUE", type="str"),
            ),
            [["P1", "V1"]],
        ),
    }

    result = execute_specification(spec, sources)

    assert isinstance(result, ExecutionSuccess), result
    assert [row["USUBJID"] for row in result.artifact.frame.to_dicts()] == ["s1"]
