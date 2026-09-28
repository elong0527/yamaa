from __future__ import annotations

from yamaa.expressions import FailedResolution, ResolvedValue, evaluate_expression
from yamaa.io.polars import frame_from_values, runtime_rows
from yamaa.models import (
    MISSING,
    ConditionResult,
    DateValue,
    TypedColumn,
    TypedTable,
    ValueResult,
)
from yamaa.odm import BindingIndex, build_binding_plan
from yamaa.specification.models import (
    Column,
    ColumnType,
    DatasetSource,
    Output,
    Specification,
)


def _table(
    names: list[str],
    rows: list[list[object]],
    types: dict[str, ColumnType] | None = None,
) -> TypedTable:
    declared_types = types or {}
    columns = tuple(
        TypedColumn(name=name, type=declared_types.get(name, "str")) for name in names
    )
    return frame_from_values(columns, rows)


def _specification(column_names: list[str]) -> Specification:
    return Specification(
        schema_version="1.0",
        domain="OUT",
        input={"ODM": DatasetSource(path="odm.csv")},
        keys=[column_names[0]],
        output=Output(path="out.csv", columns=column_names),
        columns=[Column(name=name, type="str", label=name) for name in column_names],
    )


def _index(
    table: TypedTable,
    *,
    output_columns: list[str] | None = None,
) -> BindingIndex:
    specification = _specification(output_columns or ["OUT"])
    sources = {"ODM": table}
    return BindingIndex(build_binding_plan(specification, sources), sources)


def test_direct_dataset_and_completed_output_names_resolve() -> None:
    table = _table(
        ["StudyOID", "ItemOID", "Value"],
        [["S1", "IT.TEST.TARGET", "source"]],
    )
    index = _index(table, output_columns=["OUT", "EARLIER"])
    row = runtime_rows(table)[0]
    context = index.context({"ODM": row}, {"EARLIER": "completed"})

    assert context.resolve("ODM.StudyOID") == ResolvedValue(value="S1")
    assert context.resolve("EARLIER") == ResolvedValue(value="completed")


def test_unknown_names_and_item_oids_are_failures() -> None:
    # REQ-1265: an item is read with `odm`. A suffix that names no field of
    # a long-form relation is not an ItemOID reference, whatever the
    # relation's context columns.
    table = _table(
        ["StudyOID", "SubjectKey", "ItemOID", "Value"],
        [["S1", "001", "IT.TEST.TARGET", "source"]],
    )
    index = _index(table, output_columns=["OUT", "KNOWN"])
    context = index.context({"ODM": runtime_rows(table)[0]})

    for variable in ("UNKNOWN", "KNOWN", "ODM.Unknown", "ODM.IT.TEST.TARGET"):
        result = context.resolve(variable)
        assert isinstance(result, FailedResolution)
        assert result.condition.condition == "unknown_field"
        assert result.condition.context == {"identifier": variable}

    handled = evaluate_expression(
        {"source": {"variable": "ODM.IT.TEST.TARGET", "absent": "fallback"}},
        context,
    )
    assert isinstance(handled, ConditionResult)
    assert handled.condition.condition == "unknown_field"


def _paired(options: dict[str, object]):
    table = _table(
        ["StudyOID", "ItemOID", "Value", "Rank"],
        [["S1", "IT.TEST.VALUE", "first", 1]],
        types={"Rank": "int"},
    )
    context = _index(table).context({"ODM": runtime_rows(table)[0]})
    return evaluate_expression(
        {"source": {"variable": "ODM.Value", **options}}, context
    )


def test_source_order_by_without_keep_is_unpaired() -> None:
    result = _paired(
        {"order_by": [{"variable": "ODM.Rank", "direction": "asc", "nulls": "last"}]}
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "unpaired_fields"
    assert result.condition.context["missing"] == ["keep"]


def test_source_keep_without_order_by_is_unpaired() -> None:
    result = _paired({"keep": "first"})

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "unpaired_fields"
    assert result.condition.context["missing"] == ["order_by"]


def test_one_value_on_several_records_of_a_key_reads_as_that_value() -> None:
    # REQ-0044 counts values, not records: the visit date is collected on
    # every item record of the subject, and two datings of one day are one
    # value under REQ-0573.
    table = _table(
        ["StudyOID", "SubjectKey", "VISITDT", "ItemOID", "Value"],
        [
            ["S1", "001", "2025-01-02", "IT.A", "a"],
            ["S1", "001", "2025-01-02", "IT.B", "b"],
        ],
        {"VISITDT": "date"},
    )
    feeding = runtime_rows(table)
    context = _index(table).context({"ODM": feeding[0]}, feeding_rows={"ODM": feeding})

    result = evaluate_expression({"source": "ODM.VISITDT"}, context)

    assert result == ValueResult(value=DateValue(year=2025, month=1, day=2))


def test_two_values_on_the_records_of_a_key_fail_and_count_the_values() -> None:
    table = _table(
        ["StudyOID", "SubjectKey", "VISITDT", "ItemOID", "Value"],
        [
            ["S1", "001", "2025-01-02", "IT.A", "a"],
            ["S1", "001", "2025-01-03", "IT.B", "b"],
        ],
        {"VISITDT": "date"},
    )
    feeding = runtime_rows(table)
    context = _index(table).context({"ODM": feeding[0]}, feeding_rows={"ODM": feeding})

    result = evaluate_expression({"source": "ODM.VISITDT"}, context)

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "multiple_values_per_key"
    assert result.condition.requirement == "REQ-0075"
    assert result.condition.context == {
        "identifier": "ODM.VISITDT",
        "value_count": 2,
    }


def _collected_items() -> TypedTable:
    return _table(
        ["StudyOID", "SubjectKey", "ItemOID", "Value"],
        [
            ["S1", "001", "IT.DM.SEX", "Male"],
            ["S1", "001", "IT.DM.AGE", "34"],
            ["S1", "001", "IT.DM.ARM", "Placebo"],
        ],
    )


def test_a_filter_selects_which_records_of_the_key_a_source_reads() -> None:
    # REQ-0131: the records of one key carry three collected values, and the
    # filter is what leaves the derivation the one it asks for.
    table = _collected_items()
    feeding = runtime_rows(table)
    context = _index(table).context({"ODM": feeding[0]}, feeding_rows={"ODM": feeding})

    result = evaluate_expression(
        {"source": {"variable": "ODM.Value", "filter": "ODM.ItemOID = 'IT.DM.AGE'"}},
        context,
    )

    assert result == ValueResult(value="34")


def test_an_unfiltered_read_of_those_records_still_counts_every_value() -> None:
    table = _collected_items()
    feeding = runtime_rows(table)
    context = _index(table).context({"ODM": feeding[0]}, feeding_rows={"ODM": feeding})

    result = evaluate_expression({"source": "ODM.Value"}, context)

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "multiple_values_per_key"


def test_a_filter_leaving_two_values_fails_as_that_count() -> None:
    table = _table(
        ["StudyOID", "SubjectKey", "ItemOID", "Value"],
        [
            ["S1", "001", "IT.DM.SEX", "Male"],
            ["S1", "001", "IT.DM.SEX", "Female"],
        ],
    )
    feeding = runtime_rows(table)
    context = _index(table).context({"ODM": feeding[0]}, feeding_rows={"ODM": feeding})

    result = evaluate_expression(
        {"source": {"variable": "ODM.Value", "filter": "ODM.ItemOID = 'IT.DM.SEX'"}},
        context,
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "multiple_values_per_key"
    assert result.condition.context == {"identifier": "ODM.Value", "value_count": 2}


def test_a_filter_selecting_no_record_is_missing_and_fires_no_handler() -> None:
    # REQ-0355: the subject was not asked this item, which is an absent match.
    table = _collected_items()
    feeding = runtime_rows(table)
    context = _index(table).context({"ODM": feeding[0]}, feeding_rows={"ODM": feeding})

    result = evaluate_expression(
        {
            "source": {
                "variable": "ODM.Value",
                "filter": "ODM.ItemOID = 'IT.DM.RACE'",
                "absent": "fallback",
            }
        },
        context,
    )

    assert result == ValueResult(value=MISSING)


def test_a_mapping_reads_the_records_its_own_filter_selects() -> None:
    table = _collected_items()
    feeding = runtime_rows(table)
    context = _index(table).context({"ODM": feeding[0]}, feeding_rows={"ODM": feeding})

    result = evaluate_expression(
        {
            "mapping": {
                "source": {
                    "variable": "ODM.Value",
                    "filter": "ODM.ItemOID = 'IT.DM.SEX'",
                },
                "dict": {"Male": "M", "Female": "F"},
                "missing": "U",
            }
        },
        context,
    )

    assert result == ValueResult(value="M")


def test_a_first_available_source_states_the_records_it_reads() -> None:
    table = _collected_items()
    feeding = runtime_rows(table)
    context = _index(table).context({"ODM": feeding[0]}, feeding_rows={"ODM": feeding})

    present = evaluate_expression(
        {
            "first_available": {
                "sources": [
                    {"variable": "ODM.Value", "filter": "ODM.ItemOID = 'IT.DM.ARM'"}
                ],
                "missing": "Unassigned",
            }
        },
        context,
    )
    absent = evaluate_expression(
        {
            "first_available": {
                "sources": [
                    {"variable": "ODM.Value", "filter": "ODM.ItemOID = 'IT.DM.RACE'"}
                ],
                "missing": "Unassigned",
            }
        },
        context,
    )

    assert present == ValueResult(value="Placebo")
    assert absent == ValueResult(value="Unassigned")


def test_a_filter_naming_another_relation_reads_no_record() -> None:
    table = _collected_items()
    feeding = runtime_rows(table)
    context = _index(table).context({"ODM": feeding[0]}, feeding_rows={"ODM": feeding})

    result = evaluate_expression(
        {"source": {"variable": "ODM.Value", "filter": "OTHER.ItemOID = 'IT.DM.AGE'"}},
        context,
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.condition == "unknown_field"
