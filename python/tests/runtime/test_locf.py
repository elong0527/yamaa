"""Scalar LOCF and correlated record selection share explicit observed inputs."""

from pathlib import Path

import polars as pl
import pytest

from yamaa.io import render_csv
from yamaa.io.project import ProjectResources
from yamaa.io.source import load_source_tables
from yamaa.models import TypedColumn, TypedTable
from yamaa.runtime import ExecutionFailure, ExecutionSuccess, execute_specification
from yamaa.specification import load_specification
from yamaa.specification.models import (
    DatasetSource,
    Expression,
    HandledExpression,
    Row,
)

ROOT = Path(__file__).parents[3]


def fixture(name):
    directory = ROOT / "benchmarks" / name
    spec = load_specification(directory / "spec.yaml", ROOT / "yaml").specification
    return spec, load_source_tables(spec.input, ProjectResources(directory))


def derive(value):
    return HandledExpression(value=Expression(root=value))


@pytest.mark.parametrize("name", ["adam-advs-locf", "adam-advs-locf-record"])
def test_committed_locf_examples(name):
    spec, sources = fixture(name)
    result = execute_specification(spec, sources)
    assert isinstance(result, ExecutionSuccess), result
    assert (
        render_csv(result.artifact)
        == (ROOT / "benchmarks" / name / "expected" / "advs.csv").read_bytes()
    )


def record_spec(inline=False, row_mode=None):
    spec, sources = fixture("adam-advs-locf-record")
    if inline:
        donor = spec.intermediates[0]
        columns = []
        for column in spec.columns:
            if column.name in {"AVAL", "ADT", "QSSEQ"}:
                column = column.model_copy(
                    update={
                        "derivation": derive(
                            {
                                "lookup": {
                                    "dataset": "OBS",
                                    "key": ["USUBJID", "PARAMCD"],
                                    "filter": donor.filter,
                                    "order_by": ["OBS.AVISITN", "OBS.QSSEQ"],
                                    "keep": "last",
                                    "value": column.name,
                                }
                            }
                        )
                    }
                )
            columns.append(column)
        spec = spec.model_copy(update={"columns": columns, "intermediates": []})
    if row_mode:
        derivations = {column.name: column.derivation for column in spec.columns}
        row = Row(
            id="planned",
            dataset="PLAN",
            derivations=derivations,
            group_by=["PLAN.USUBJID", "PLAN.PARAMCD", "PLAN.AVISITN"]
            if row_mode == "grouped"
            else None,
        )
        spec = spec.model_copy(
            update={
                # Explicit row driver takes precedence over this unrelated base.
                "base": "OBS",
                "rows": [row],
                "columns": [
                    column.model_copy(update={"derivation": None})
                    for column in spec.columns
                ],
            }
        )
    return spec, sources


@pytest.mark.parametrize("inline", [False, True])
@pytest.mark.parametrize("row_mode", [None, "record", "grouped"])
def test_correlated_lookup_in_every_row_context(inline, row_mode):
    spec, sources = record_spec(inline, row_mode)
    result = execute_specification(spec, sources)
    assert isinstance(result, ExecutionSuccess), result
    assert (
        render_csv(result.artifact)
        == (ROOT / "benchmarks/adam-advs-locf-record/expected/advs.csv").read_bytes()
    )


def replace_filter(spec, predicate):
    if spec.intermediates:
        return spec.model_copy(
            update={
                "intermediates": [
                    spec.intermediates[0].model_copy(update={"filter": predicate})
                ]
            }
        )
    columns = []
    for column in spec.columns:
        if column.name in {"AVAL", "ADT", "QSSEQ"}:
            payload = dict(column.derivation.value.root["lookup"])
            payload["filter"] = predicate
            column = column.model_copy(
                update={"derivation": derive({"lookup": payload})}
            )
        columns.append(column)
    return spec.model_copy(update={"columns": columns})


@pytest.mark.parametrize("inline", [False, True])
def test_strictly_prior_filter_excludes_current_and_future_observations(inline):
    spec, sources = record_spec(inline)
    spec = replace_filter(
        spec,
        "OBS.ANL01FL = 'Y' AND OBS.AVAL IS NOT NULL AND OBS.AVISITN < PLAN.AVISITN",
    )
    result = execute_specification(spec, sources)
    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame["AVAL"].to_list() == [None, None, 5, 5, 5, None, None]


@pytest.mark.parametrize("inline", [False, True])
@pytest.mark.parametrize(
    "predicate",
    [
        "OBS.AVISITN < AVISITN",
        "OBS.AVISITN < PLAN.ABSENT",
        "OBS.AVISITN < OTHER.AVISITN",
    ],
)
def test_filter_rejects_unqualified_unknown_or_non_driver_references(inline, predicate):
    spec, sources = record_spec(inline)
    spec = spec.model_copy(
        update={
            "input": {
                **spec.input,
                "OTHER": DatasetSource(path="input/plan.csv"),
            }
        }
    )
    sources = {**sources, "OTHER": sources["PLAN"]}
    result = execute_specification(replace_filter(spec, predicate), sources)
    assert isinstance(result, ExecutionFailure)
    assert any(d.condition == "unknown_field" for d in result.diagnostics)


@pytest.mark.parametrize("inline", [False, True])
def test_grouped_filter_cannot_read_a_varying_driver_field(inline):
    spec, sources = record_spec(inline, "grouped")
    row = spec.rows[0]
    # Grouping deliberately omits the field the correlated predicate reads.
    row = row.model_copy(update={"group_by": ["PLAN.USUBJID", "PLAN.PARAMCD"]})
    result = execute_specification(spec.model_copy(update={"rows": [row]}), sources)
    assert isinstance(result, ExecutionFailure)
    assert any(
        d.condition in {"ungrouped_driver_field", "phase_boundary"}
        for d in result.diagnostics
    )


@pytest.mark.parametrize("inline", [False, True])
def test_missing_target_comparison_yields_no_donor(inline):
    spec, sources = record_spec(inline)
    # A non-key cutoff can be missing without invalidating the output row key.
    plan = sources["PLAN"].table
    sources["PLAN"] = TypedTable(
        columns=(*plan.columns, TypedColumn(name="CUTOFF", type="int")),
        frame=plan.frame.with_columns(pl.lit(None, dtype=pl.Int64).alias("CUTOFF")),
    )
    result = execute_specification(
        replace_filter(spec, "OBS.AVISITN < PLAN.CUTOFF"), sources
    )
    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame["AVAL"].to_list() == [None] * 7


@pytest.mark.parametrize("inline", [False, True])
def test_filter_rejects_incomparable_values(inline):
    spec, sources = record_spec(inline)
    result = execute_specification(
        replace_filter(spec, "OBS.AVISITN < PLAN.USUBJID"), sources
    )
    assert isinstance(result, ExecutionFailure)
    assert any(d.condition == "incompatible_input_type" for d in result.diagnostics)


def test_locf_window_filter_excludes_donors_and_targets():
    spec, sources = fixture("adam-advs-locf")
    column = spec.columns[-1]
    payload = dict(column.derivation.value.root["locf"])
    payload["window"] = {**payload["window"], "filter": "AVISITN <> 1"}
    spec = spec.model_copy(
        update={
            "columns": [
                *spec.columns[:-1],
                column.model_copy(update={"derivation": derive({"locf": payload})}),
            ]
        }
    )
    result = execute_specification(spec, sources)
    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame["AVAL"].to_list() == [
        None,
        None,
        None,
        None,
        6,
        None,
        0,
        0,
        None,
    ]


def test_locf_cannot_read_its_own_output():
    spec, sources = fixture("adam-advs-locf")
    column = spec.columns[-1]
    payload = {**column.derivation.value.root["locf"], "source": "AVAL"}
    spec = spec.model_copy(
        update={
            "columns": [
                *spec.columns[:-1],
                column.model_copy(update={"derivation": derive({"locf": payload})}),
            ]
        }
    )
    result = execute_specification(spec, sources)
    assert isinstance(result, ExecutionFailure)
    assert any(
        "cycl" in d.condition or "self" in d.condition for d in result.diagnostics
    )
