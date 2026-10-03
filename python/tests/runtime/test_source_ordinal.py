from __future__ import annotations

from pathlib import Path

import pytest
import yaml

from yamaa.io import ProjectResources, load_source_tables
from yamaa.runtime import ExecutionSuccess, execute_with_source_provider
from yamaa.specification import SpecificationError, load_specification

ROOT = Path(__file__).parents[3]
BENCHMARK = ROOT / "benchmarks/schema-source-ordinal"


def _spec() -> dict:
    return yaml.safe_load((BENCHMARK / "spec.yaml").read_text())


def _run(tmp_path: Path, spec: dict, records: str | None = None):
    (tmp_path / "input").mkdir(exist_ok=True)
    (tmp_path / "input/source.csv").write_text(
        records or (BENCHMARK / "input/source.csv").read_text()
    )
    (tmp_path / "spec.yaml").write_text(yaml.safe_dump(spec, sort_keys=False))
    loaded = load_specification(tmp_path / "spec.yaml", ROOT / "yaml")
    resources = ProjectResources(tmp_path)
    result = execute_with_source_provider(
        loaded.specification,
        lambda datasets: load_source_tables(datasets, resources),
    )
    assert isinstance(result, ExecutionSuccess), result
    return result.artifact.frame


def test_filter_keeps_positions_and_numbers_identical_records(tmp_path: Path) -> None:
    assert _run(tmp_path, _spec()).rows() == [
        ("G1", 1, "ROW_B", 2),
        ("G1", 2, "ROW_A", 5),
        ("G1", 3, "ROW_A", 6),
        ("G2", 1, "ROW_B", 1),
        ("G2", 2, "ROW_A", 4),
    ]


def test_ordinal_orders_a_record_driven_base_without_row_templates(tmp_path: Path):
    spec = _spec()
    del spec["rows"]
    assert _run(tmp_path, spec).rows()[:4] == [
        ("G1", 1, "ROW_B", 2),
        ("G1", 2, "SKIPPED", 3),
        ("G1", 3, "ROW_A", 5),
        ("G1", 4, "ROW_A", 6),
    ]


@pytest.mark.parametrize("labels", [("ROW_B", "ROW_A"), ("ROW_A", "ROW_B")])
def test_swapping_physical_records_swaps_sequence_assignment(tmp_path: Path, labels):
    spec = _spec()
    records = "GroupID,RecordLabel,Include\n" + "".join(
        f"G1,{label},Y\n" for label in labels
    )
    assert _run(tmp_path, spec, records).rows() == [
        ("G1", 1, labels[0], 1),
        ("G1", 2, labels[1], 2),
    ]


def test_generated_field_is_not_automatically_emitted(tmp_path: Path) -> None:
    spec = _spec()
    spec["output"]["columns"].remove("SOURCE_ORDINAL")
    spec["columns"] = [
        col for col in spec["columns"] if col["name"] != "SOURCE_ORDINAL"
    ]
    frame = _run(tmp_path, spec)
    assert frame.columns == ["GROUP_ID", "ROW_SEQUENCE", "RECORD_LABEL"]
    assert frame.rows()[0] == ("G1", 1, "ROW_B")


def test_grouped_template_can_reduce_and_order_by_the_ordinal(tmp_path: Path) -> None:
    spec = _spec()
    spec["keys"] = ["GROUP_ID"]
    spec["output"]["columns"] = ["GROUP_ID", "SOURCE_ORDINAL", "ROW_SEQUENCE"]
    spec["columns"] = [col for col in spec["columns"] if col["name"] != "RECORD_LABEL"]
    next(col for col in spec["columns"] if col["name"] == "SOURCE_ORDINAL")[
        "derivation"
    ] = {"aggregate": "MIN(SOURCE.SourceOrdinal)"}
    next(col for col in spec["columns"] if col["name"] == "ROW_SEQUENCE")[
        "derivation"
    ] = {"row_number": {"window": {"order_by": ["SOURCE_ORDINAL"]}}}
    spec["rows"][0]["group_by"] = ["SOURCE.GroupID"]
    del spec["rows"][0]["filter"]
    for col in spec["columns"]:
        if col["name"] in {"SOURCE_ORDINAL", "ROW_SEQUENCE"}:
            spec["rows"][0]["derivations"][col["name"]] = col.pop("derivation")
    assert _run(tmp_path, spec).rows() == [("G1", 2, 2), ("G2", 1, 1)]


def test_multiple_templates_retain_the_same_input_positions(tmp_path: Path) -> None:
    spec = _spec()
    spec["keys"] = ["GROUP_ID", "ROW_SEQUENCE"]
    spec["rows"].append(
        {
            "id": "second",
            "dataset": "SOURCE",
            "filter": "SOURCE.SourceOrdinal = 1",
            "derivations": {},
        }
    )
    assert _run(tmp_path, spec).rows()[-3:] == [
        ("G2", 1, "ROW_B", 1),
        ("G2", 2, "ROW_B", 1),
        ("G2", 3, "ROW_A", 4),
    ]


@pytest.mark.parametrize("ordinal", ["", "SOURCE.Position", "1Position", 1, None])
def test_ordinal_declaration_requires_an_identifier(tmp_path: Path, ordinal) -> None:
    spec = _spec()
    spec["input"]["SOURCE"]["ordinal"] = ordinal
    (tmp_path / "spec.yaml").write_text(yaml.safe_dump(spec, sort_keys=False))
    with pytest.raises(SpecificationError):
        load_specification(tmp_path / "spec.yaml", ROOT / "yaml")
