from __future__ import annotations

from pathlib import Path

from yamaa.expressions import FailedResolution, ResolvedValue, evaluate_expression
from yamaa.io import ProjectResources, load_source_table, load_source_tables
from yamaa.io.polars import frame_from_values, runtime_rows
from yamaa.models import MISSING, ConditionResult, TypedColumn, TypedTable, ValueResult
from yamaa.odm import ODM_CONTEXT_COLUMNS, BindingIndex, build_binding_plan
from yamaa.specification import load_specification
from yamaa.specification.models import (
    Column,
    ColumnType,
    DatasetSource,
    Output,
    Specification,
)

REPOSITORY = Path(__file__).parents[3]


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
        datasets={"ODM": DatasetSource(path="odm.csv")},
        keys=[column_names[0]],
        output=Output(path="out.csv", columns=column_names),
        columns=[Column(name=name, type="str", label=name) for name in column_names],
    )


def _index(
    table: TypedTable,
    *,
    batch_size: int | None = None,
    output_columns: list[str] | None = None,
) -> BindingIndex:
    specification = _specification(output_columns or ["OUT"])
    sources = {"ODM": table}
    return BindingIndex(
        build_binding_plan(specification, sources),
        sources,
        batch_size=batch_size,
    )


def test_form_scoped_fixture_resolves_only_the_current_form() -> None:
    root = REPOSITORY / "yaml/examples/odm-form-scoped-item-resolution"
    loaded_spec = load_specification(root / "spec.yaml", REPOSITORY / "yaml")
    sources = load_source_tables(
        loaded_spec.specification.datasets,
        ProjectResources(root),
    )
    index = BindingIndex(
        build_binding_plan(loaded_spec.specification, sources),
        sources,
        batch_size=3,
    )
    rows = [
        row
        for row in runtime_rows(sources["ODM"].table)
        if row["ItemOID"] == "IT.LB.RESULT"
    ]
    expression = next(
        column.derivation.value
        for column in loaded_spec.specification.columns
        if column.name == "LBDTC" and column.derivation is not None
    )

    results = [
        evaluate_expression(expression, index.context({"ODM": row})) for row in rows
    ]

    assert [result.value for result in results if isinstance(result, ValueResult)] == [
        "2025-01-02",
        "2025-01-03",
        "2025-01-04",
        "2025-01-05",
        MISSING,
    ]
    assert isinstance(results[-1], ValueResult)
    assert results[-1].handled_by == "missing"


def test_dm_fixture_resolves_contextual_age_and_arm_without_dropping_rows() -> None:
    root = REPOSITORY / "yaml/examples/sdtm-dm-basic"
    loaded_spec = load_specification(root / "spec.yaml", REPOSITORY / "yaml")
    sources = load_source_tables(
        loaded_spec.specification.datasets,
        ProjectResources(root),
    )
    index = BindingIndex(
        build_binding_plan(loaded_spec.specification, sources),
        sources,
    )
    rows = [
        row
        for row in runtime_rows(sources["ODM"].table)
        if row["ItemOID"] == "IT.DM.SEX"
    ]
    expressions = {
        column.name: column.derivation.value
        for column in loaded_spec.specification.columns
        if column.name in {"AGE", "ARM"} and column.derivation is not None
    }

    ages = [
        evaluate_expression(expressions["AGE"], index.context({"ODM": row}))
        for row in rows
    ]
    arms = [
        evaluate_expression(expressions["ARM"], index.context({"ODM": row}))
        for row in rows
    ]

    assert ages == [
        ValueResult(value="34"),
        ValueResult(value="28"),
        ValueResult(value=MISSING, handled_by="missing"),
        ValueResult(value=MISSING, handled_by="missing"),
    ]
    assert arms == [
        ValueResult(value="Placebo"),
        ValueResult(value="Vitamin D3"),
        ValueResult(value="Unassigned", handled_by="missing"),
        ValueResult(value="Unassigned", handled_by="missing"),
    ]


def test_every_available_context_level_is_part_of_the_index_key() -> None:
    base = ["S1", "M1", "P1", "E1", "1", "F1", "1", "G1", "1"]
    contexts = [base]
    for position, replacement in enumerate(
        ["S2", "M2", "P2", "E2", "2", "F2", "2", "G2", "2"]
    ):
        changed = list(base)
        changed[position] = replacement
        contexts.append(changed)
    missing_repeats = list(base)
    for position in (4, 6, 8):
        missing_repeats[position] = None  # type: ignore[assignment]
    contexts.append(missing_repeats)

    names = [*ODM_CONTEXT_COLUMNS, "ItemOID", "Value"]
    rows: list[list[object]] = []
    for index, context in enumerate(contexts):
        rows.append([*context, "IT.TEST.TARGET", f"target-{index}"])
        rows.append([*context, "IT.TEST.VALUE", f"value-{index}"])
    table = _table(names, rows)
    binding_index = _index(table)

    for expected, target in enumerate(runtime_rows(table)[::2]):
        resolved = binding_index.context({"ODM": target}).resolve("ODM.IT.TEST.VALUE")
        assert resolved == ResolvedValue(value=f"value-{expected}")


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


def test_period_free_odm_item_oid_resolves_contextually() -> None:
    table = _table(
        ["StudyOID", "ItemOID", "Value"],
        [
            ["S1", "IT.TEST.TARGET", "target"],
            ["S1", "AGE", "42"],
        ],
    )
    context = _index(table).context({"ODM": runtime_rows(table)[0]})

    assert context.resolve("ODM.AGE") == ResolvedValue(value="42")


def test_unknown_names_and_item_references_without_context_are_failures() -> None:
    table = _table(
        ["ItemOID", "Value"],
        [["IT.TEST.TARGET", "source"]],
    )
    index = _index(table, output_columns=["OUT", "KNOWN"])
    context = index.context({"ODM": runtime_rows(table)[0]})

    for variable in ("UNKNOWN", "KNOWN", "ODM.Unknown", "ODM.IT.TEST.VALUE"):
        result = context.resolve(variable)
        assert isinstance(result, FailedResolution)
        assert result.condition.condition == "unknown_field"
        assert result.condition.context == {"identifier": variable}

    handled = evaluate_expression(
        {"source": {"variable": "ODM.IT.TEST.VALUE", "missing": "fallback"}},
        context,
    )
    assert isinstance(handled, ConditionResult)
    assert handled.condition.condition == "unknown_field"


def test_absent_item_and_matched_missing_value_take_different_paths(
    tmp_path: Path,
) -> None:
    (tmp_path / "odm.csv").write_bytes(
        b"StudyOID,ItemOID,Value\n"
        b"S1,IT.TEST.TARGET,target\n"
        b'S1,IT.TEST.VALUE,""\n'
        b"S2,IT.TEST.TARGET,target\n"
    )
    loaded = load_source_table(
        "ODM",
        DatasetSource(path="odm.csv"),
        ProjectResources(tmp_path),
    )
    index = _index(loaded.table)
    first, _, second = runtime_rows(loaded.table)
    expression = {"source": {"variable": "ODM.IT.TEST.VALUE", "missing": "fallback"}}

    present_missing = evaluate_expression(
        expression,
        index.context({"ODM": first}),
    )
    absent = evaluate_expression(expression, index.context({"ODM": second}))

    assert present_missing == ValueResult(value=MISSING)
    assert absent == ValueResult(value="fallback", handled_by="missing")


def test_duplicate_context_requires_or_reports_multiple_match_selection() -> None:
    table = _table(
        ["StudyOID", "ItemOID", "Value", "Rank", "Include"],
        [
            ["S1", "IT.TEST.TARGET", "target", None, None],
            ["S1", "IT.TEST.VALUE", "excluded", 0, "N"],
            ["S1", "IT.TEST.VALUE", "first", 1, "Y"],
            ["S1", "IT.TEST.VALUE", "last", 1, "Y"],
        ],
        types={"Rank": "int"},
    )
    index = _index(table)
    target = runtime_rows(table)[0]
    context = index.context({"ODM": target})

    duplicate = evaluate_expression({"source": "ODM.IT.TEST.VALUE"}, context)
    assert isinstance(duplicate, ConditionResult)
    assert duplicate.condition.phase == "join"
    assert duplicate.condition.condition == "multiple_matches"
    assert duplicate.condition.applicable_handler == "multiple_matches"

    first = evaluate_expression(
        {
            "source": {
                "variable": "ODM.IT.TEST.VALUE",
                "multiple_matches": {
                    "filter": "ODM.Include = 'Y'",
                    "order_by": [
                        {
                            "variable": "ODM.Rank",
                            "direction": "asc",
                            "nulls": "last",
                        }
                    ],
                    "keep": "first",
                },
            }
        },
        context,
    )
    last = evaluate_expression(
        {
            "source": {
                "variable": "ODM.IT.TEST.VALUE",
                "multiple_matches": {
                    "filter": "ODM.Include = 'Y'",
                    "order_by": [
                        {
                            "variable": "ODM.Rank",
                            "direction": "asc",
                            "nulls": "last",
                        }
                    ],
                    "keep": "last",
                },
            }
        },
        context,
    )

    assert first == ValueResult(value="first", handled_by="multiple_matches")
    assert last == ValueResult(value="last", handled_by="multiple_matches")


def test_multiple_match_count_requires_more_than_one_filtered_survivor() -> None:
    table = _table(
        ["StudyOID", "ItemOID", "Value", "Rank"],
        [
            ["S1", "IT.TEST.TARGET", "target", None],
            ["S1", "IT.TEST.VALUE", "first", 1],
            ["S1", "IT.TEST.VALUE", "second", 2],
        ],
        types={"Rank": "int"},
    )
    context = _index(table).context({"ODM": runtime_rows(table)[0]})
    policy = {
        "order_by": [{"variable": "ODM.Rank", "direction": "asc", "nulls": "last"}],
        "keep": "first",
    }

    one = evaluate_expression(
        {
            "source": {
                "variable": "ODM.IT.TEST.VALUE",
                "multiple_matches": {**policy, "filter": "ODM.Rank = 2"},
            }
        },
        context,
    )
    none = evaluate_expression(
        {
            "source": {
                "variable": "ODM.IT.TEST.VALUE",
                "missing": "fallback",
                "multiple_matches": {**policy, "filter": "ODM.Rank > 9"},
            }
        },
        context,
    )

    assert one == ValueResult(value="second")
    assert none == ValueResult(value=MISSING)


def test_order_terms_are_validated_when_filter_leaves_one_survivor() -> None:
    table = _table(
        ["StudyOID", "ItemOID", "Value", "Include"],
        [
            ["S1", "IT.TEST.TARGET", "target", None],
            ["S1", "IT.TEST.VALUE", "kept", "Y"],
            ["S1", "IT.TEST.VALUE", "excluded", "N"],
        ],
    )
    context = _index(table).context({"ODM": runtime_rows(table)[0]})

    result = evaluate_expression(
        {
            "source": {
                "variable": "ODM.IT.TEST.VALUE",
                "multiple_matches": {
                    "filter": "ODM.Include = 'Y'",
                    "order_by": [
                        {
                            "variable": "ODM.Unknown",
                            "direction": "asc",
                            "nulls": "last",
                        }
                    ],
                    "keep": "first",
                },
            }
        },
        context,
    )

    assert isinstance(result, ConditionResult)
    assert result.condition.phase == "validation"
    assert result.condition.condition == "unknown_field"
    assert result.condition.context == {"identifier": "ODM.Unknown"}


def test_multiple_match_filter_also_applies_to_one_contextual_match() -> None:
    table = _table(
        ["StudyOID", "ItemOID", "Value", "Include"],
        [
            ["S1", "IT.TEST.TARGET", "target", None],
            ["S1", "IT.TEST.VALUE", "only", "N"],
        ],
    )
    context = _index(table).context({"ODM": runtime_rows(table)[0]})
    result = evaluate_expression(
        {
            "source": {
                "variable": "ODM.IT.TEST.VALUE",
                "missing": "fallback",
                "multiple_matches": {
                    "filter": "ODM.Include = 'Y'",
                    "order_by": [
                        {
                            "variable": "ODM.Include",
                            "direction": "asc",
                            "nulls": "last",
                        }
                    ],
                    "keep": "first",
                },
            }
        },
        context,
    )

    assert result == ValueResult(value=MISSING)


def test_duplicate_order_applies_direction_and_null_placement_independently() -> None:
    table = _table(
        ["StudyOID", "ItemOID", "Value", "Rank"],
        [
            ["S1", "IT.TEST.TARGET", "target", 0],
            ["S1", "IT.TEST.VALUE", "missing-rank", None],
            ["S1", "IT.TEST.VALUE", "rank-one", 1],
            ["S1", "IT.TEST.VALUE", "rank-two", 2],
        ],
        types={"Rank": "int"},
    )
    context = _index(table).context({"ODM": runtime_rows(table)[0]})

    def choose(nulls: str) -> ValueResult | ConditionResult:
        return evaluate_expression(
            {
                "source": {
                    "variable": "ODM.IT.TEST.VALUE",
                    "multiple_matches": {
                        "order_by": [
                            {
                                "variable": "ODM.Rank",
                                "direction": "desc",
                                "nulls": nulls,
                            }
                        ],
                        "keep": "first",
                    },
                }
            },
            context,
        )

    assert choose("first") == ValueResult(
        value="missing-rank", handled_by="multiple_matches"
    )
    assert choose("last") == ValueResult(
        value="rank-two", handled_by="multiple_matches"
    )


def test_index_batching_preserves_source_order_tie_breaks() -> None:
    table = _table(
        ["StudyOID", "ItemOID", "Value", "Rank"],
        [
            ["S1", "IT.TEST.TARGET", "target", 0],
            ["S1", "IT.TEST.VALUE", "one", 1],
            ["S1", "IT.TEST.VALUE", "two", 1],
            ["S1", "IT.TEST.VALUE", "three", 1],
        ],
        types={"Rank": "int"},
    )
    target = runtime_rows(table)[0]
    expression = {
        "source": {
            "variable": "ODM.IT.TEST.VALUE",
            "multiple_matches": {
                "order_by": [
                    {
                        "variable": "ODM.Rank",
                        "direction": "asc",
                        "nulls": "last",
                    }
                ],
                "keep": "last",
            },
        }
    }

    results = [
        evaluate_expression(
            expression,
            _index(table, batch_size=batch_size).context({"ODM": target}),
        )
        for batch_size in (None, 1, 2, 3, 20)
    ]

    assert results == [ValueResult(value="three", handled_by="multiple_matches")] * 5
