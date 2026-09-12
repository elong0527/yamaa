from __future__ import annotations

from pathlib import Path

import polars as pl
import pytest

from yamaa.io import (
    ProjectResources,
    SourceDiagnostic,
    SourceError,
    build_artifact,
    load_source_tables,
    render_artifact,
)
from yamaa.models import TypedColumn, TypedTable
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionHooks,
    ExecutionSuccess,
    ExecutionUnsupported,
    execute_specification,
    execute_with_source_provider,
)
from yamaa.specification import SpecificationError, load_specification
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    Output,
    Row,
    Specification,
)
from yamaa.verification import check_column, check_dataset, check_keys

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"
EXAMPLES = SCHEMA_ROOT / "examples"
DM_EXAMPLE = EXAMPLES / "sdtm-dm-basic"


def dm_inputs() -> tuple[object, dict[str, object]]:
    specification = load_specification(
        DM_EXAMPLE / "spec.yaml", SCHEMA_ROOT
    ).specification
    resources = ProjectResources(DM_EXAMPLE)
    return specification, load_source_tables(specification.datasets, resources)


def test_the_basic_dm_specification_derives_four_ordered_typed_rows() -> None:
    specification = load_specification(
        DM_EXAMPLE / "spec.yaml", SCHEMA_ROOT
    ).specification
    resources = ProjectResources(DM_EXAMPLE)

    result = execute_with_source_provider(
        specification,
        lambda datasets: load_source_tables(datasets, resources),
    )

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.to_dicts() == [
        {
            "STUDYID": "STUDY01",
            "DOMAIN": "DM",
            "USUBJID": "001",
            "SUBJID": "001",
            "SEX": "M",
            "AGE": 34,
            "ARM": "Placebo",
            "ACTARM": "Placebo",
        },
        {
            "STUDYID": "STUDY01",
            "DOMAIN": "DM",
            "USUBJID": "002",
            "SUBJID": "002",
            "SEX": "F",
            "AGE": 28,
            "ARM": "Vitamin D3",
            "ACTARM": "Vitamin D3",
        },
        {
            "STUDYID": "STUDY01",
            "DOMAIN": "DM",
            "USUBJID": "003",
            "SUBJID": "003",
            "SEX": "U",
            "AGE": None,
            "ARM": "Unassigned",
            "ACTARM": "Unassigned",
        },
        {
            "STUDYID": "STUDY01",
            "DOMAIN": "DM",
            "USUBJID": "004",
            "SUBJID": "004",
            "SEX": "U",
            "AGE": None,
            "ARM": "Unassigned",
            "ACTARM": "Unassigned",
        },
    ]
    assert result.artifact.frame.schema["AGE"] == pl.Int64
    assert (
        render_artifact(result.artifact)
        == (DM_EXAMPLE / "expected/dm.csv").read_bytes()
    )
    assert [(item.spec_path, item.count) for item in result.handler_counts] == [
        ("columns.SEX.derivation.mapping.missing", 1),
        ("columns.SEX.derivation.mapping.unmapped", 1),
        ("columns.AGE.derivation.source.missing", 2),
        ("columns.ARM.derivation.source.missing", 2),
    ]


def test_execution_uses_source_values_and_never_needs_expected_artifacts() -> None:
    specification, loaded = dm_inputs()
    source = loaded["ODM"]
    changed = source.table.frame.with_columns(
        pl.when((pl.col("SubjectKey") == "001") & (pl.col("ItemOID") == "IT.DM.SEX"))
        .then(pl.lit("Female"))
        .when((pl.col("SubjectKey") == "001") & (pl.col("ItemOID") == "IT.DM.AGE"))
        .then(pl.lit("35"))
        .otherwise(pl.col("Value"))
        .alias("Value")
    )
    sources = {"ODM": TypedTable(columns=source.table.columns, frame=changed)}

    result = execute_specification(specification, sources)

    assert isinstance(result, ExecutionSuccess)
    first = result.artifact.frame.row(0, named=True)
    assert first["SEX"] == "F"
    assert first["AGE"] == 35


def test_column_enrichment_keeps_the_constructed_row_count() -> None:
    specification, sources = dm_inputs()
    observed: list[tuple[str, int]] = []

    def observe(table, column, keys):
        observed.append((column.name, table.frame.height))
        return check_column(table, column, keys)

    result = execute_specification(
        specification,
        sources,
        hooks=ExecutionHooks(column=observe),
    )

    assert isinstance(result, ExecutionSuccess)
    assert [name for name, _ in observed] == [
        "STUDYID",
        "DOMAIN",
        "USUBJID",
        "SUBJID",
        "SEX",
        "AGE",
        "ARM",
        "ACTARM",
    ]
    assert {height for _, height in observed} == {4}


def test_verification_and_output_hooks_run_in_normative_order() -> None:
    specification, sources = dm_inputs()
    observed: list[str] = []

    def observe_column(table, column, keys):
        observed.append(f"column:{column.name}")
        return check_column(table, column, keys)

    def observe_keys(table, keys):
        observed.append("keys")
        return check_keys(table, keys)

    def observe_dataset(table, verifications, keys):
        observed.append("dataset")
        return check_dataset(table, verifications, keys)

    def observe_output(table, output, keys):
        observed.append("output")
        return build_artifact(table, output, keys)

    result = execute_specification(
        specification,
        sources,
        hooks=ExecutionHooks(
            column=observe_column,
            keys=observe_keys,
            dataset=observe_dataset,
            output=observe_output,
        ),
    )

    assert isinstance(result, ExecutionSuccess)
    assert observed == [
        "column:STUDYID",
        "column:DOMAIN",
        "column:USUBJID",
        "column:SUBJID",
        "column:SEX",
        "column:AGE",
        "column:ARM",
        "column:ACTARM",
        "keys",
        "dataset",
        "output",
    ]


def test_row_templates_and_driver_records_keep_their_declared_order() -> None:
    def derive(expression):
        return HandledExpression(value=Expression(root=expression))

    specification = Specification(
        schema_version="1.0",
        domain="OUT",
        datasets={"SRC": DatasetSource(path="input/source.csv")},
        base="SRC",
        keys=["KIND", "VALUE"],
        output=Output(path="out.csv", columns=["KIND", "VALUE"]),
        columns=[
            Column(name="KIND", type="str"),
            Column(name="VALUE", type="str", derivation=derive({"source": "SRC.X"})),
        ],
        rows=[
            Row(
                id="selected",
                filter="SRC.X = 'two'",
                derivations={"KIND": derive({"literal": "selected"})},
            ),
            Row(
                id="all",
                derivations={"KIND": derive({"literal": "all"})},
            ),
        ],
    )
    sources = {
        "SRC": TypedTable(
            columns=(TypedColumn(name="X", type="str"),),
            frame=pl.DataFrame({"X": ["one", "two"]}, schema={"X": pl.String}),
        )
    }

    result = execute_specification(specification, sources)

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [
        ("selected", "two"),
        ("all", "one"),
        ("all", "two"),
    ]


def test_results_and_handler_counts_are_deterministic() -> None:
    specification, sources = dm_inputs()

    first = execute_specification(specification, sources)
    second = execute_specification(specification, sources)

    assert isinstance(first, ExecutionSuccess)
    assert isinstance(second, ExecutionSuccess)
    assert first.artifact.frame.equals(second.artifact.frame)
    assert first.handler_counts == second.handler_counts


def test_valid_but_unimplemented_operations_have_unsupported_status() -> None:
    specification, sources = dm_inputs()
    age = specification.columns[5]
    unsupported_age = age.model_copy(
        update={
            "derivation": age.derivation.model_copy(
                update={
                    "value": age.derivation.value.model_copy(
                        update={"root": {"str_upper": {"source": "ODM.Value"}}}
                    )
                }
            )
        }
    )
    changed = specification.model_copy(
        update={
            "columns": [
                *specification.columns[:5],
                unsupported_age,
                *specification.columns[6:],
            ]
        }
    )

    result = execute_specification(changed, sources)

    assert isinstance(result, ExecutionUnsupported)
    assert result.features[0].operation == "str_upper"


def test_invalid_column_type_fails_before_any_source_is_ingested() -> None:
    example = EXAMPLES / "negative-column-type-unknown"
    resources = ProjectResources(example)

    with pytest.raises(SpecificationError) as raised:
        load_specification(example / "spec.yaml", SCHEMA_ROOT)

    assert raised.value.diagnostics[0].model_dump(mode="json") == {
        "phase": "validation",
        "condition": "value_not_permitted",
        "spec_paths": ["columns.AVAL.type"],
        "context": {
            "value": "number",
            "permitted": ["str", "int", "float", "date", "datetime"],
        },
    }
    assert resources.capture_reads == 0


def test_output_dataset_self_reference_fails_before_ingestion() -> None:
    example = EXAMPLES / "negative-source-output-self-reference"
    resources = ProjectResources(example)
    specification = load_specification(example / "spec.yaml", SCHEMA_ROOT).specification
    provider_called = False

    def provide_sources(datasets):
        nonlocal provider_called
        provider_called = True
        return load_source_tables(datasets, resources)

    result = execute_with_source_provider(specification, provide_sources)

    assert isinstance(result, ExecutionFailure)
    assert result.diagnostics[0].condition == "duplicate_identifier"
    assert result.diagnostics[0].spec_paths == ("datasets.ADLB", "domain")
    assert result.diagnostics[0].context == {"identifier": "ADLB"}
    assert not provider_called
    assert resources.capture_reads == 0


def test_source_provider_diagnostics_enter_the_execution_result() -> None:
    specification = load_specification(
        DM_EXAMPLE / "spec.yaml", SCHEMA_ROOT
    ).specification

    def fail_to_load(_datasets):
        raise SourceError(
            [
                SourceDiagnostic(
                    phase="ingest",
                    condition="resource_changed",
                    spec_paths=("datasets.ODM.path",),
                    context={"dataset": "ODM"},
                )
            ]
        )

    result = execute_with_source_provider(specification, fail_to_load)

    assert isinstance(result, ExecutionFailure)
    assert result.diagnostics[0].model_dump(mode="json") == {
        "phase": "ingest",
        "condition": "resource_changed",
        "spec_paths": ["datasets.ODM.path"],
        "context": {"dataset": "ODM"},
    }
    assert result.handler_counts == ()
