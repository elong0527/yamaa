"""Tests for R024 value-level submission metadata (issue #565)."""

from __future__ import annotations

import copy
from pathlib import Path

import pytest
import yaml

from yamaa.specification import SpecificationError, load_specification
from yamaa.specification.submission import (
    has_value_level_metadata,
    validate_value_level_metadata,
    warn_value_level_define_deferred,
)


def _base_document() -> dict:
    return {
        "schema_version": "1.0",
        "domain": "LB",
        "input": {"LB_RAW": {"path": "lb_raw.csv"}},
        "keys": ["STUDYID", "USUBJID", "LBTESTCD"],
        "submission": {
            "label": "Laboratory",
            "class": "FINDINGS",
            "structure": "One record per lab test per subject",
            "repeating": True,
        },
        "output": {
            "path": "lb.csv",
            "columns": ["STUDYID", "USUBJID", "LBTESTCD", "LBORRESU"],
        },
        "columns": [
            {
                "name": "STUDYID",
                "type": "str",
                "derivation": {"source": "LB_RAW.STUDYID"},
            },
            {
                "name": "USUBJID",
                "type": "str",
                "derivation": {"source": "LB_RAW.USUBJID"},
            },
            {
                "name": "LBTESTCD",
                "type": "str",
                "derivation": {"source": "LB_RAW.LBTESTCD"},
            },
            {
                "name": "LBORRESU",
                "type": "str",
                "derivation": {"source": "LB_RAW.LBORRESU"},
                "submission": {
                    "origin": {"type": "Collected", "source": "Vendor"},
                    "values": [
                        {"testcd": "GLUC", "codelist": "CL_UNIT_GLUCOSE"},
                        {"testcd": "HGB", "codelist": "CL_UNIT_HGB"},
                    ],
                },
            },
        ],
    }


def _column(document: dict, name: str) -> dict:
    return next(item for item in document["columns"] if item["name"] == name)


def test_positive_document_passes() -> None:
    document = _base_document()
    assert validate_value_level_metadata(document) == []
    assert has_value_level_metadata(document) is True


def test_no_values_declared_is_quiet() -> None:
    document = _base_document()
    del _column(document, "LBORRESU")["submission"]
    assert validate_value_level_metadata(document) == []
    assert has_value_level_metadata(document) is False


def test_values_outside_output_columns_fails() -> None:
    document = _base_document()
    document["output"]["columns"].remove("LBORRESU")
    (diagnostic,) = validate_value_level_metadata(document)
    assert diagnostic.condition == "values_outside_output_columns"
    assert diagnostic.requirement == "R024-76"


def test_values_requires_findings_class() -> None:
    document = _base_document()
    document["submission"]["class"] = "EVENTS"
    (diagnostic,) = validate_value_level_metadata(document)
    assert diagnostic.condition == "values_requires_findings_class"
    assert diagnostic.requirement == "R024-77"


def test_duplicate_testcd_fails() -> None:
    document = _base_document()
    values = _column(document, "LBORRESU")["submission"]["values"]
    values.append({"testcd": "GLUC"})
    (diagnostic,) = validate_value_level_metadata(document)
    assert diagnostic.condition == "duplicate_value_testcd"
    assert diagnostic.requirement == "R024-78"
    assert diagnostic.context["testcd"] == "GLUC"


def test_entry_data_type_must_be_admitted() -> None:
    document = _base_document()
    values = _column(document, "LBORRESU")["submission"]["values"]
    values[0]["data_type"] = "integer"
    (diagnostic,) = validate_value_level_metadata(document)
    assert diagnostic.condition == "submission_data_type_not_admitted"
    assert diagnostic.requirement == "R024-79"


def test_entry_derived_origin_requires_method() -> None:
    document = _base_document()
    values = _column(document, "LBORRESU")["submission"]["values"]
    values[0]["origin"] = {"type": "Derived", "source": "Sponsor"}
    conditions = [item.condition for item in validate_value_level_metadata(document)]
    # The bare-source derivation also refutes the Derived origin outright.
    assert conditions == ["method_missing", "origin_contradicts_derivation"]


def test_entry_origin_refuted_by_bare_source() -> None:
    document = _base_document()
    values = _column(document, "LBORRESU")["submission"]["values"]
    values[0]["origin"] = {"type": "Derived", "source": "Sponsor"}
    values[0]["method"] = "Coding applied after collection"
    (diagnostic,) = validate_value_level_metadata(document)
    assert diagnostic.condition == "origin_contradicts_derivation"
    assert diagnostic.requirement == "R024-79"


def test_entry_inherits_column_origin_and_method() -> None:
    document = _base_document()
    column = _column(document, "LBORRESU")
    column["derivation"] = {
        "str_concat": {"sources": [{"source": "LB_RAW.LBORRESU"}, {"literal": "x"}]}
    }
    column["submission"]["origin"] = {"type": "Derived", "source": "Sponsor"}
    column["submission"]["method"] = "Unit appended during derivation"
    assert validate_value_level_metadata(document) == []


def test_missing_testcd_column_fails() -> None:
    document = _base_document()
    document["output"]["columns"].remove("LBTESTCD")
    (diagnostic,) = validate_value_level_metadata(document)
    assert diagnostic.condition == "value_testcd_column_missing"
    assert diagnostic.requirement == "R024-80"
    assert diagnostic.context["testcd_column"] == "LBTESTCD"


def test_values_reserved_in_freeform_metadata() -> None:
    document = _base_document()
    _column(document, "LBORRESU")["metadata"] = {"values": "something"}
    conditions = [item.condition for item in validate_value_level_metadata(document)]
    assert "reserved_metadata_key" in conditions


def test_deferred_notice_warns() -> None:
    with pytest.warns(UserWarning, match="not yet implemented"):
        warn_value_level_define_deferred()


def test_loader_emits_deferred_notice(tmp_path) -> None:
    schema_root = Path(__file__).parents[3] / "yaml"
    document = _base_document()
    spec_path = tmp_path / "spec.yaml"
    spec_path.write_text(yaml.safe_dump(document), encoding="utf-8")
    with pytest.warns(UserWarning, match="not yet implemented"):
        loaded = load_specification(spec_path, schema_root)
    assert loaded.specification.domain == "LB"


def test_loader_rejects_duplicate_testcd(tmp_path) -> None:
    schema_root = Path(__file__).parents[3] / "yaml"
    document = _base_document()
    values = _column(document, "LBORRESU")["submission"]["values"]
    values.append({"testcd": "GLUC"})
    spec_path = tmp_path / "spec.yaml"
    spec_path.write_text(yaml.safe_dump(document), encoding="utf-8")
    with pytest.raises(SpecificationError) as error:
        load_specification(spec_path, schema_root)
    assert error.value.diagnostics[0].condition == "duplicate_value_testcd"


def test_base_document_is_not_mutated_by_validation() -> None:
    document = _base_document()
    snapshot = copy.deepcopy(document)
    validate_value_level_metadata(document)
    assert document == snapshot


def _write_parents_pair(tmp_path, parent_values) -> Path:
    parent = _base_document()
    _column(parent, "LBORRESU")["submission"]["values"] = parent_values
    (tmp_path / "parent.yaml").write_text(yaml.safe_dump(parent), encoding="utf-8")
    child = tmp_path / "spec.yaml"
    child.write_text(
        'schema_version: "1.0"\n'
        "parents: parent.yaml\n"
        "output:\n"
        "  path: lb.csv\n"
        "  columns: [STUDYID, USUBJID, LBTESTCD, LBORRESU]\n",
        encoding="utf-8",
    )
    return child


def test_loader_validates_value_level_metadata_through_parents(tmp_path) -> None:
    schema_root = Path(__file__).parents[3] / "yaml"
    child = _write_parents_pair(tmp_path, [{"testcd": "GLUC"}, {"testcd": "GLUC"}])
    with pytest.raises(SpecificationError) as error:
        load_specification(child, schema_root)
    assert error.value.diagnostics[0].condition == "duplicate_value_testcd"


def test_loader_warns_for_inherited_value_level_metadata(tmp_path) -> None:
    schema_root = Path(__file__).parents[3] / "yaml"
    child = _write_parents_pair(tmp_path, [{"testcd": "GLUC"}])
    with pytest.warns(UserWarning, match="value-level submission metadata"):
        loaded = load_specification(child, schema_root)
    assert loaded.specification.domain == "LB"
