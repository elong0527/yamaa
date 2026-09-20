from __future__ import annotations

from pathlib import Path

import polars as pl
import pytest
import yaml

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
EXAMPLES = REPOSITORY_ROOT / "benchmarks"
DM_EXAMPLE = EXAMPLES / "sdtm-dm-basic"
WARNING_EXAMPLE = EXAMPLES / "adam-adsl-age-quality-review"


def dm_inputs() -> tuple[object, dict[str, object]]:
    specification = load_specification(
        DM_EXAMPLE / "spec.yaml", SCHEMA_ROOT
    ).specification
    resources = ProjectResources(DM_EXAMPLE)
    return specification, load_source_tables(specification.input, resources)


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
            "ARMNRS": None,
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
            "ARMNRS": None,
        },
        {
            "STUDYID": "STUDY01",
            "DOMAIN": "DM",
            "USUBJID": "003",
            "SUBJID": "003",
            "SEX": "U",
            "AGE": None,
            "ARM": None,
            "ACTARM": None,
            "ARMNRS": "Not assigned to treatment arm",
        },
        {
            "STUDYID": "STUDY01",
            "DOMAIN": "DM",
            "USUBJID": "004",
            "SUBJID": "004",
            "SEX": "U",
            "AGE": None,
            "ARM": None,
            "ACTARM": None,
            "ARMNRS": "Not assigned to treatment arm",
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
    ]


def test_warning_verification_keeps_the_artifact_and_builds_the_exact_log() -> None:
    specification = load_specification(
        WARNING_EXAMPLE / "spec.yaml", SCHEMA_ROOT
    ).specification
    resources = ProjectResources(WARNING_EXAMPLE)

    result = execute_with_source_provider(
        specification,
        lambda datasets: load_source_tables(datasets, resources),
    )

    assert isinstance(result, ExecutionSuccess)
    assert (
        render_artifact(result.artifact)
        == (WARNING_EXAMPLE / "expected/adsl.csv").read_bytes()
    )
    assert len(result.warnings) == 1
    assert result.warnings[0].severity == "warning"
    assert result.warnings[0].offending_keys == (
        {"STUDYID": "PILOT7", "USUBJID": "P7-732"},
    )
    assert result.violation_log is not None
    assert (
        render_artifact(result.violation_log)
        == (WARNING_EXAMPLE / "expected/adsl-violations.csv").read_bytes()
    )


def test_declared_violation_log_is_header_only_when_no_warning_fires() -> None:
    specification = load_specification(
        WARNING_EXAMPLE / "spec.yaml", SCHEMA_ROOT
    ).specification
    resources = ProjectResources(WARNING_EXAMPLE)
    loaded = load_source_tables(specification.input, resources)["DM"]
    sources = {
        "DM": TypedTable(
            columns=loaded.table.columns,
            frame=loaded.table.frame.with_columns(pl.lit(40).alias("AGE")),
        )
    }

    result = execute_specification(specification, sources)

    assert isinstance(result, ExecutionSuccess)
    assert result.warnings == ()
    assert result.violation_log is not None
    expected = (
        (WARNING_EXAMPLE / "expected/adsl-violations.csv")
        .read_bytes()
        .splitlines(keepends=True)[0]
    )
    assert render_artifact(result.violation_log) == expected


def test_warning_without_violation_log_fails_before_source_ingestion() -> None:
    specification = load_specification(
        WARNING_EXAMPLE / "spec.yaml", SCHEMA_ROOT
    ).specification
    changed = specification.model_copy(
        update={
            "output": specification.output.model_copy(update={"violation_log": None})
        }
    )
    provider_called = False

    def provide_sources(_datasets):
        nonlocal provider_called
        provider_called = True
        return {}

    result = execute_with_source_provider(changed, provide_sources)

    assert isinstance(result, ExecutionFailure)
    assert result.diagnostics[0].condition == "missing_violation_log"
    assert result.diagnostics[0].spec_paths == ("output.violation_log",)
    assert result.diagnostics[0].requirement == "REQ-0391"
    assert not provider_called


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
        "DOMAIN",
        "STUDYID",
        "USUBJID",
        "SUBJID",
        "SEX",
        "AGE",
        "ARM",
        "ACTARM",
        "ARMNRS",
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
        "column:DOMAIN",
        "column:STUDYID",
        "column:USUBJID",
        "column:SUBJID",
        "column:SEX",
        "column:AGE",
        "column:ARM",
        "column:ACTARM",
        "column:ARMNRS",
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
        input={"SRC": DatasetSource(path="input/source.csv")},
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


def test_key_grain_without_rows_emits_one_row_per_key_combination() -> None:
    def derive(expression):
        return HandledExpression(value=Expression(root=expression))

    specification = Specification(
        schema_version="1.0",
        domain="OUT",
        input={"SRC": DatasetSource(path="input/source.csv")},
        base="SRC",
        keys=["VALUE"],
        output=Output(path="out.csv", columns=["VALUE", "TAG"]),
        columns=[
            Column(name="VALUE", type="str", derivation=derive({"source": "SRC.X"})),
            Column(name="TAG", type="str", derivation=derive({"literal": "kept"})),
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
    assert result.artifact.frame.rows() == [("one", "kept"), ("two", "kept")]


def test_key_grain_reads_a_field_constant_over_the_records_of_one_key() -> None:
    def derive(expression):
        return HandledExpression(value=Expression(root=expression))

    specification = Specification(
        schema_version="1.0",
        domain="OUT",
        input={"SRC": DatasetSource(path="input/source.csv")},
        base="SRC",
        keys=["GRP"],
        output=Output(path="out.csv", columns=["GRP", "LABEL"]),
        columns=[
            Column(name="GRP", type="str", derivation=derive({"source": "SRC.G"})),
            Column(name="LABEL", type="str", derivation=derive({"source": "SRC.L"})),
        ],
    )
    sources = {
        "SRC": TypedTable(
            columns=(
                TypedColumn(name="G", type="str"),
                TypedColumn(name="L", type="str"),
            ),
            frame=pl.DataFrame(
                # Three records of one subject repeat the subject's label.
                {"G": ["one", "one", "one"], "L": ["kept", "kept", "kept"]},
                schema={"G": pl.String, "L": pl.String},
            ),
        )
    }

    result = execute_specification(specification, sources)

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [("one", "kept")]


def test_key_grain_without_rows_rejects_multiple_values_per_key() -> None:
    def derive(expression):
        return HandledExpression(value=Expression(root=expression))

    specification = Specification(
        schema_version="1.0",
        domain="OUT",
        input={"SRC": DatasetSource(path="input/source.csv")},
        base="SRC",
        keys=["GRP"],
        output=Output(path="out.csv", columns=["GRP", "VALUE"]),
        columns=[
            Column(name="GRP", type="str", derivation=derive({"source": "SRC.G"})),
            Column(name="VALUE", type="str", derivation=derive({"source": "SRC.X"})),
        ],
    )
    sources = {
        "SRC": TypedTable(
            columns=(
                TypedColumn(name="G", type="str"),
                TypedColumn(name="X", type="str"),
            ),
            frame=pl.DataFrame(
                {"G": ["one", "one"], "X": ["1", "2"]},
                schema={"G": pl.String, "X": pl.String},
            ),
        )
    }

    result = execute_specification(specification, sources)

    assert isinstance(result, ExecutionFailure)
    (diagnostic,) = result.diagnostics
    assert diagnostic.phase == "derivation"
    assert diagnostic.condition == "multiple_values_per_key"
    assert diagnostic.spec_paths == ("columns.VALUE.derivation.source",)
    assert diagnostic.requirement == "REQ-0075"
    assert diagnostic.context["identifier"] == "SRC.X"
    assert diagnostic.context["value_count"] == 2
    assert diagnostic.context["keys"] == [{"GRP": "one"}]


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
                        update={"root": {"function": {"name": "f", "args": []}}}
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
    assert result.features[0].operation == "function"


def test_invalid_column_type_fails_before_any_source_is_ingested() -> None:
    example = EXAMPLES / "negative-column-type-unknown"
    resources = ProjectResources(example)

    with pytest.raises(SpecificationError) as raised:
        load_specification(example / "spec.yaml", SCHEMA_ROOT)

    assert raised.value.diagnostics[0].model_dump(mode="json") == {
        "phase": "validation",
        "condition": "value_not_permitted",
        "spec_paths": ["columns.AVAL.type"],
        "requirement": "REQ-0012",
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
    assert result.diagnostics[0].spec_paths == ("input.ADLB", "domain")
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
                    spec_paths=("input.ODM.path",),
                    context={"dataset": "ODM"},
                )
            ]
        )

    result = execute_with_source_provider(specification, fail_to_load)

    assert isinstance(result, ExecutionFailure)
    assert result.diagnostics[0].model_dump(mode="json") == {
        "phase": "ingest",
        "condition": "resource_changed",
        "spec_paths": ["input.ODM.path"],
        "requirement": None,
        "context": {"dataset": "ODM"},
    }
    assert result.handler_counts == ()


# Committed key contracts: how many rows a specification emits is the
# declared keys' answer (REQ-0042), so the two ways a key combination can
# still come out wrong each keep an example pinning the error it raises.
# test_examples.py compares every negative example's `requirement`; these
# two also pin the phase, condition, spec paths and reported keys, which is
# what tells the output gate and the derivation failure apart.
@pytest.mark.parametrize(
    "name",
    ["negative-output-duplicate-subject", "negative-keys-conflicting-values"],
)
def test_a_committed_grain_error_contract_is_reproduced(name: str) -> None:
    directory = EXAMPLES / name
    committed = yaml.safe_load(
        (directory / "expected" / "error.yaml").read_text("utf-8")
    )
    specification = load_specification(
        directory / "spec.yaml", SCHEMA_ROOT
    ).specification
    resources = ProjectResources(directory)

    result = execute_with_source_provider(
        specification,
        lambda datasets: load_source_tables(datasets, resources),
    )

    assert isinstance(result, ExecutionFailure)
    diagnostic = result.diagnostics[0]
    assert diagnostic.phase == committed["phase"]
    assert diagnostic.condition == committed["condition"]
    assert list(diagnostic.spec_paths) == committed["spec_paths"]
    assert diagnostic.requirement == committed["requirement"]
    assert committed["context"].items() <= diagnostic.context.items()


def test_window_key_numbers_partitions_after_scalar_keys() -> None:
    def derive(expression):
        return HandledExpression(value=Expression(root=expression))

    specification = Specification(
        schema_version="1.0",
        domain="OUT",
        input={"SRC": DatasetSource(path="input/source.csv")},
        base="SRC",
        keys=["GRP", "SEQ"],
        output=Output(path="out.csv", columns=["GRP", "SEQ"]),
        columns=[
            Column(name="GRP", type="str", derivation=derive({"source": "SRC.G"})),
            Column(
                name="SEQ",
                type="int",
                derivation=derive(
                    {
                        "row_number": {
                            "window": {
                                "group_by": ["GRP"],
                                "order_by": ["SRC.X"],
                            }
                        }
                    }
                ),
            ),
        ],
    )
    sources = {
        "SRC": TypedTable(
            columns=(
                TypedColumn(name="G", type="str"),
                TypedColumn(name="X", type="str"),
            ),
            frame=pl.DataFrame(
                {"G": ["a", "a", "b"], "X": ["2", "1", "1"]},
                schema={"G": pl.String, "X": pl.String},
            ),
        )
    }

    result = execute_specification(specification, sources)

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [("a", 2), ("a", 1), ("b", 1)]


def test_key_plan_sees_earlier_key_values() -> None:
    def derive(expression):
        return HandledExpression(value=Expression(root=expression))

    specification = Specification(
        schema_version="1.0",
        domain="OUT",
        input={"SRC": DatasetSource(path="input/source.csv")},
        base="SRC",
        keys=["GRP", "TAG"],
        output=Output(path="out.csv", columns=["GRP", "TAG"]),
        columns=[
            Column(name="GRP", type="str", derivation=derive({"source": "SRC.G"})),
            Column(name="TAG", type="str", derivation=derive({"source": "GRP"})),
        ],
    )
    sources = {
        "SRC": TypedTable(
            columns=(TypedColumn(name="G", type="str"),),
            frame=pl.DataFrame({"G": ["one", "two"]}, schema={"G": pl.String}),
        )
    }

    result = execute_specification(specification, sources)

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [("one", "one"), ("two", "two")]


def test_a_root_filter_keeps_only_matching_driver_records() -> None:
    def derive(expression):
        return HandledExpression(value=Expression(root=expression))

    specification = Specification(
        schema_version="1.0",
        domain="OUT",
        input={"SRC": DatasetSource(path="input/source.csv")},
        base="SRC",
        keys=["X"],
        output=Output(path="out.csv", columns=["X"]),
        columns=[
            Column(name="X", type="str", derivation=derive({"source": "SRC.X"})),
        ],
        filter="SRC.X <> 'two'",
    )
    sources = {
        "SRC": TypedTable(
            columns=(TypedColumn(name="X", type="str"),),
            frame=pl.DataFrame({"X": ["one", "two", "three"]}, schema={"X": pl.String}),
        )
    }

    result = execute_specification(specification, sources)

    assert isinstance(result, ExecutionSuccess)
    assert result.artifact.frame.rows() == [("one",), ("three",)]
