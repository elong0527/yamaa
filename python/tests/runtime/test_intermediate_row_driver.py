"""A filtered intermediate can supply the records a row template emits."""

from __future__ import annotations

from pathlib import Path

import pytest

from yamaa.io import ProjectResources, load_source_tables
from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.runtime import (
    ExecutionFailure,
    ExecutionSuccess,
    execute_specification,
    execute_with_source_provider,
)
from yamaa.specification import load_specification
from yamaa.specification.models import (
    Column,
    DatasetSource,
    Expression,
    HandledExpression,
    Intermediate,
    OrderTerm,
    Output,
    Row,
    Specification,
)


def derive(expression: dict[str, object]) -> HandledExpression:
    return HandledExpression(value=Expression(root=expression))


def test_an_authored_yaml_spec_uses_a_named_intermediate_row_driver(
    tmp_path: Path,
) -> None:
    (tmp_path / "input.csv").write_text("ID,SEQ\nA,1\nA,2\nB,3\n", encoding="ascii")
    (tmp_path / "spec.yaml").write_text(
        """schema_version: "1.0"
domain: OUT
keys: [ID]
input:
  SRC:
    path: input.csv
    types: {ID: str, SEQ: int}
intermediates:
  - id: TOP
    dataset: SRC
    derivations:
      _RN:
        row_number:
          window:
            group_by: [ID]
            order_by: [{variable: SEQ, direction: desc}]
    filter: "SRC._RN = 1"
    columns: [ID, SEQ]
rows:
  - id: chosen
    dataset: TOP
    derivations:
      ID: {source: TOP.ID}
      SEQ: {source: TOP.SEQ}
columns:
  - {name: ID, type: str}
  - {name: SEQ, type: int}
output: {path: out.csv, columns: [ID, SEQ]}
""",
        encoding="ascii",
    )
    specification = load_specification(
        tmp_path / "spec.yaml", Path(__file__).parents[3] / "yaml"
    ).specification
    resources = ProjectResources(tmp_path)

    result = execute_with_source_provider(
        specification, lambda datasets: load_source_tables(datasets, resources)
    )

    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame.to_dicts() == [
        {"ID": "A", "SEQ": 2},
        {"ID": "B", "SEQ": 3},
    ]


def example() -> tuple[Specification, object]:
    fields = (
        TypedColumn(name="STUDYID", type="str"),
        TypedColumn(name="USUBJID", type="str"),
        TypedColumn(name="LBTESTCD", type="str"),
        TypedColumn(name="LBSEQ", type="int"),
        TypedColumn(name="VISITNUM", type="int"),
        TypedColumn(name="LBSTRESN", type="float"),
    )
    source = frame_from_values(
        fields,
        [
            ["S", "01", "CHOL", 1, 1, 10.0],
            ["S", "01", "GLUC", 2, 2, 20.0],
            ["S", "01", "GLUC", 3, 3, 30.0],
            ["S", "02", "CHOL", 4, 4, 40.0],
            ["S", "01", "CHOL", 5, 9, 90.0],
            ["S", "03", "CHOL", 6, 12, 60.0],
        ],
    )
    intermediate = Intermediate(
        id="EOTFB",
        dataset="LB",
        derivations={
            "_EOT_AWARE": derive(
                {
                    "case": [
                        {
                            "when": "LB.VISITNUM = 9",
                            "then": {"literal": 99},
                        },
                        {"otherwise": {"source": "LB.VISITNUM"}},
                    ]
                }
            ),
            "_RN": derive(
                {
                    "row_number": {
                        "window": {
                            "group_by": ["STUDYID", "USUBJID", "LBTESTCD"],
                            "order_by": [
                                {"variable": "_EOT_AWARE", "direction": "desc"},
                                {"variable": "LBSEQ"},
                            ],
                        }
                    }
                }
            ),
        },
        filter="LB._RN = 1 AND LB._EOT_AWARE <> 99",
        columns=["STUDYID", "USUBJID", "LBTESTCD", "LBSEQ", "VISITNUM", "LBSTRESN"],
    )
    specification = Specification(
        schema_version="1.0",
        domain="ADLBC",
        input={"LB": DatasetSource(path="input/lb.csv")},
        keys=["STUDYID", "USUBJID", "LBTESTCD"],
        output=Output(
            path="out.csv",
            columns=["STUDYID", "USUBJID", "LBTESTCD", "LBSEQ", "AVAL"],
        ),
        columns=[
            Column(name="STUDYID", type="str"),
            Column(name="USUBJID", type="str"),
            Column(name="LBTESTCD", type="str"),
            Column(name="LBSEQ", type="int"),
            Column(name="AVAL", type="float"),
        ],
        intermediates=[intermediate],
        rows=[
            Row(
                id="eot2",
                dataset="EOTFB",
                filter="EOTFB.VISITNUM <> 12",
                derivations={
                    "STUDYID": derive({"source": "EOTFB.STUDYID"}),
                    "USUBJID": derive({"source": "EOTFB.USUBJID"}),
                    "LBTESTCD": derive({"source": "EOTFB.LBTESTCD"}),
                    "LBSEQ": derive({"source": "EOTFB.LBSEQ"}),
                    "AVAL": derive({"source": "EOTFB.LBSTRESN"}),
                },
            )
        ],
    )
    return specification, source


def test_rank_then_filter_intermediate_drives_rows_in_source_order() -> None:
    specification, source = example()

    result = execute_specification(specification, {"LB": source})

    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame.to_dicts() == [
        {"STUDYID": "S", "USUBJID": "01", "LBTESTCD": "GLUC", "LBSEQ": 3, "AVAL": 30.0},
        {"STUDYID": "S", "USUBJID": "02", "LBTESTCD": "CHOL", "LBSEQ": 4, "AVAL": 40.0},
    ]


def test_row_driver_needs_no_applicable_lookup_key() -> None:
    specification, source = example()
    row = specification.rows[0]
    row = row.model_copy(
        update={
            "derivations": {
                "K": derive({"source": "EOTFB.LBSEQ"}),
                **row.derivations,
            }
        }
    )
    specification = specification.model_copy(
        update={
            "keys": ["K"],
            "columns": [Column(name="K", type="int"), *specification.columns],
            "rows": [row],
            "output": specification.output.model_copy(
                update={"columns": ["K", *specification.output.columns]}
            ),
        }
    )

    result = execute_specification(specification, {"LB": source})

    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame["K"].to_list() == [3, 4]


def test_a_projected_derived_field_is_read_from_the_driver_record() -> None:
    specification, source = example()
    intermediate = specification.intermediates[0].model_copy(
        update={
            "columns": [
                *specification.intermediates[0].columns,
                "_EOT_AWARE",
                "_RN",
            ]
        }
    )
    row = specification.rows[0].model_copy(
        update={
            "derivations": {
                **specification.rows[0].derivations,
                "RN": derive({"source": "EOTFB._RN"}),
                "AWARE": derive({"source": "EOTFB._EOT_AWARE"}),
            }
        }
    )
    specification = specification.model_copy(
        update={
            "intermediates": [intermediate],
            "rows": [row],
            "columns": [
                *specification.columns,
                Column(name="RN", type="int"),
                Column(name="AWARE", type="int"),
            ],
            "output": specification.output.model_copy(
                update={"columns": [*specification.output.columns, "RN", "AWARE"]}
            ),
        }
    )

    result = execute_specification(specification, {"LB": source})

    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame["RN"].to_list() == [1, 1]
    assert result.artifact.frame["AWARE"].to_list() == [3, 4]


def test_grouped_rows_partition_the_filtered_intermediate_records() -> None:
    specification, source = example()
    row = specification.rows[0].model_copy(
        update={
            "group_by": ["EOTFB.STUDYID", "EOTFB.USUBJID"],
            "filter": None,
            "derivations": {
                "STUDYID": derive({"source": "EOTFB.STUDYID"}),
                "USUBJID": derive({"source": "EOTFB.USUBJID"}),
            },
        }
    )
    specification = specification.model_copy(
        update={
            "keys": ["STUDYID", "USUBJID"],
            "rows": [row],
            "columns": specification.columns[:2],
            "output": specification.output.model_copy(
                update={"columns": ["STUDYID", "USUBJID"]}
            ),
        }
    )

    result = execute_specification(specification, {"LB": source})

    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame.to_dicts() == [
        {"STUDYID": "S", "USUBJID": "01"},
        {"STUDYID": "S", "USUBJID": "02"},
        {"STUDYID": "S", "USUBJID": "03"},
    ]


def test_another_lookup_can_match_against_the_intermediate_driver() -> None:
    specification, source = example()
    prior = Intermediate(
        id="PRIOR",
        dataset="LB",
        key=["USUBJID", "LBTESTCD"],
        key_base=["EOTFB.USUBJID", "EOTFB.LBTESTCD"],
        filter="LB.VISITNUM < EOTFB.VISITNUM",
        order_by=[OrderTerm(variable="LB.VISITNUM")],
        keep="last",
        columns=["LBSTRESN"],
    )
    row = specification.rows[0].model_copy(
        update={
            "derivations": {
                **specification.rows[0].derivations,
                "PREV": derive({"source": "PRIOR.LBSTRESN"}),
            }
        }
    )
    specification = specification.model_copy(
        update={
            "intermediates": [*specification.intermediates, prior],
            "rows": [row],
            "columns": [*specification.columns, Column(name="PREV", type="float")],
            "output": specification.output.model_copy(
                update={"columns": [*specification.output.columns, "PREV"]}
            ),
        }
    )

    result = execute_specification(specification, {"LB": source})

    assert isinstance(result, ExecutionSuccess), result
    assert result.artifact.frame["PREV"].to_list() == [20.0, None]


@pytest.mark.parametrize(
    "change, field",
    [
        ({"missing": "NA"}, "absence_policy"),
        ({"dataset": "SELF"}, "dataset"),
        ({"key": ["STUDYID"]}, "key"),
        ({"keep": "first", "order_by": [{"variable": "LB.LBSEQ"}]}, "keep"),
    ],
)
def test_a_row_driver_rejects_row_dependent_selection(change: dict, field: str) -> None:
    specification, source = example()
    intermediate = Intermediate.model_validate(
        {**specification.intermediates[0].model_dump(), **change}
    )
    specification = specification.model_copy(update={"intermediates": [intermediate]})

    result = execute_specification(specification, {"LB": source})

    assert isinstance(result, ExecutionFailure)
    assert any(
        diagnostic.condition == "invalid_intermediate_driver"
        and field in diagnostic.context["fields"]
        for diagnostic in result.diagnostics
    )


def test_a_correlated_intermediate_cannot_drive_rows() -> None:
    specification, source = example()
    intermediate = specification.intermediates[0].model_copy(
        update={"filter": "LB._RN = 1 AND LB.VISITNUM < OTHER.LBSEQ"}
    )
    specification = specification.model_copy(
        update={
            "input": {
                **specification.input,
                "OTHER": DatasetSource(path="input/other.csv"),
            },
            "intermediates": [intermediate],
        }
    )

    result = execute_specification(specification, {"LB": source, "OTHER": source})

    assert isinstance(result, ExecutionFailure)
    assert any(
        diagnostic.condition == "invalid_intermediate_driver"
        and "filter" in diagnostic.context["fields"]
        for diagnostic in result.diagnostics
    )


def test_an_exposed_derivation_needs_a_static_driver_type() -> None:
    specification, source = example()
    original = specification.intermediates[0]
    intermediate = original.model_copy(
        update={
            "derivations": {
                **original.derivations,
                "_CALC": derive({"compute": {"expr": "LB.LBSEQ + 1"}}),
            },
            "columns": [*original.columns, "_CALC"],
        }
    )
    specification = specification.model_copy(update={"intermediates": [intermediate]})

    result = execute_specification(specification, {"LB": source})

    assert isinstance(result, ExecutionFailure)
    assert any(
        diagnostic.condition == "unknown_intermediate_driver_type"
        and diagnostic.context["columns"] == ["_CALC"]
        for diagnostic in result.diagnostics
    )
