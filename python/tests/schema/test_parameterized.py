"""`for:` parameterized intermediates and row templates (REQ-1262)."""

from __future__ import annotations

from pathlib import Path

import polars as pl
import pytest
import yaml
from polars.testing import assert_frame_equal

from yamaa import yamaa_domain
from yamaa.schema.parameterized import expand_parameterized_definitions
from yamaa.specification import SpecificationError, load_specification
from yamaa.specification.schema import load_schema_bundle

ROOT = Path(__file__).parents[3]
SCHEMA = ROOT / "yaml"


def base_document():
    return {
        "schema_version": "1.0",
        "domain": "OUT",
        "input": {
            "SRC": {"path": "input.csv", "types": {"ID": "str", "V": "int", "X": "int"}}
        },
        "base": "SRC",
        "keys": ["ID", "V"],
        "output": {"path": "out.csv", "columns": ["ID", "V", "X", "Y"]},
        "columns": [
            {"name": "ID", "type": "str", "label": "ID"},
            {"name": "V", "type": "int", "label": "V"},
            {"name": "X", "type": "int", "label": "X"},
            {"name": "Y", "type": "int", "label": "Y"},
        ],
        "intermediates": [
            {
                "id": "PICK{v}",
                "for": [1, 2],
                "dataset": "SRC",
                "key": ["ID"],
                "key_base": ["SRC.ID"],
                "filter": "SRC.V = {v}",
                "columns": ["X"],
            }
        ],
        "rows": [
            {
                "id": "obs",
                "dataset": "SRC",
                "derivations": {
                    "ID": "SRC.ID",
                    "V": "SRC.V",
                    "X": "SRC.X",
                    "Y": {"literal": None},
                },
            },
            {
                "id": "shifted{v}",
                "for": [{"v": 1, "w": 11}, {"v": 2, "w": 12}],
                "dataset": "SRC",
                "filter": "SRC.V = {v}",
                "derivations": {
                    "ID": "SRC.ID",
                    "V": {"literal": "{w}"},
                    "X": "SRC.X",
                    "Y": "PICK{v}.X",
                },
            },
        ],
    }


def write_spec(tmp_path, value, name="spec.yaml"):
    path = tmp_path / name
    path.write_text(yaml.safe_dump(value, sort_keys=False), encoding="ascii")
    (tmp_path / "input.csv").write_text(
        "ID,V,X\na,1,10\na,2,20\nb,1,30\n", encoding="ascii"
    )
    return path


def test_scalar_for_expands_intermediate():
    expanded = expand_parameterized_definitions(base_document())["intermediates"]
    assert [item["id"] for item in expanded] == ["PICK1", "PICK2"]
    assert expanded[0]["filter"] == "SRC.V = 1"
    assert expanded[1]["filter"] == "SRC.V = 2"
    assert "for" not in expanded[0]


def test_mapping_for_expands_row_template_with_typed_whole_string():
    expanded = expand_parameterized_definitions(base_document())["rows"]
    assert [row["id"] for row in expanded] == ["obs", "shifted1", "shifted2"]
    first, second = expanded[1], expanded[2]
    assert first["filter"] == "SRC.V = 1"
    assert first["derivations"]["V"] == {"literal": 11}
    assert isinstance(first["derivations"]["V"]["literal"], int)
    assert first["derivations"]["Y"] == "PICK1.X"
    assert second["derivations"]["V"] == {"literal": 12}
    assert second["derivations"]["Y"] == "PICK2.X"


def test_integral_float_renders_without_fraction_in_strings():
    document = base_document()
    document["intermediates"][0]["for"] = [8.0, 16.5]
    expanded = expand_parameterized_definitions(document)["intermediates"]
    assert [item["id"] for item in expanded] == ["PICK8", "PICK16.5"]
    assert expanded[0]["filter"] == "SRC.V = 8"
    assert expanded[1]["filter"] == "SRC.V = 16.5"


def test_definitions_without_for_pass_through():
    document = base_document()
    del document["intermediates"][0]["for"]
    document["intermediates"][0]["id"] = "PICK"
    document["intermediates"][0]["filter"] = "SRC.V = 1"
    expanded = expand_parameterized_definitions(document)["intermediates"]
    assert len(expanded) == 1
    assert expanded[0]["id"] == "PICK"


def test_invalid_for_item_is_an_error():
    document = base_document()
    document["intermediates"][0]["for"] = [1, ["not", "a", "scalar"]]
    with pytest.raises(SpecificationError) as excinfo:
        expand_parameterized_definitions(document)
    diagnostics = excinfo.value.diagnostics
    assert len(diagnostics) == 1
    assert diagnostics[0].condition == "invalid_for_item"


def test_empty_for_list_is_an_error():
    document = base_document()
    document["intermediates"][0]["for"] = []
    with pytest.raises(SpecificationError) as excinfo:
        expand_parameterized_definitions(document)
    assert excinfo.value.diagnostics[0].condition == "empty_for_list"


def test_unknown_placeholder_is_an_error():
    document = base_document()
    document["intermediates"][0]["filter"] = "V = {w}"
    with pytest.raises(SpecificationError) as excinfo:
        expand_parameterized_definitions(document)
    diagnostic = excinfo.value.diagnostics[0]
    assert diagnostic.condition == "unknown_for_variable"
    assert diagnostic.context["variables"] == ["w"]


def test_parameterized_spec_loads_and_matches_handwritten_expansion(tmp_path):
    entry = write_spec(tmp_path, base_document())
    loaded = load_specification(entry, SCHEMA).specification

    handwritten = base_document()
    handwritten["intermediates"] = [
        {
            "id": "PICK1",
            "dataset": "SRC",
            "key": ["ID"],
            "key_base": ["SRC.ID"],
            "filter": "SRC.V = 1",
            "columns": ["X"],
        },
        {
            "id": "PICK2",
            "dataset": "SRC",
            "key": ["ID"],
            "key_base": ["SRC.ID"],
            "filter": "SRC.V = 2",
            "columns": ["X"],
        },
    ]
    handwritten["rows"] = [
        handwritten["rows"][0],
        {
            "id": "shifted1",
            "dataset": "SRC",
            "filter": "SRC.V = 1",
            "derivations": {
                "ID": "SRC.ID",
                "V": {"literal": 11},
                "X": "SRC.X",
                "Y": "PICK1.X",
            },
        },
        {
            "id": "shifted2",
            "dataset": "SRC",
            "filter": "SRC.V = 2",
            "derivations": {
                "ID": "SRC.ID",
                "V": {"literal": 12},
                "X": "SRC.X",
                "Y": "PICK2.X",
            },
        },
    ]
    reference = write_spec(tmp_path, handwritten, "reference.yaml")
    expected = load_specification(reference, SCHEMA).specification
    assert loaded == expected


def test_parameterized_spec_derives_expected_rows(tmp_path):
    entry = write_spec(tmp_path, base_document())
    run = yamaa_domain(entry, schema_root=SCHEMA)
    assert run.issues.is_empty()
    assert run.output is not None
    expected = pl.DataFrame(
        {
            "ID": ["a", "a", "b", "a", "b", "a"],
            "V": [1, 2, 1, 11, 11, 12],
            "X": [10, 20, 30, 10, 30, 20],
            "Y": [None, None, None, 10, 30, 20],
        },
        schema={"ID": pl.String, "V": pl.Int64, "X": pl.Int64, "Y": pl.Int64},
    )
    assert_frame_equal(run.output.sort(["ID", "V"]), expected.sort(["ID", "V"]))


def test_for_survives_schema_bundle_validation(tmp_path):
    # The `for` field itself must validate against the schema bundle.
    entry = write_spec(tmp_path, base_document())
    bundle = load_schema_bundle(SCHEMA)
    assert any("for" in field for field in bundle.classes["intermediate_class"])
    assert any("for" in field for field in bundle.classes["row_class"])
    assert load_specification(entry, SCHEMA).specification is not None


def test_str_template_collision_is_an_error():
    document = base_document()
    document["rows"][1]["derivations"]["LABEL"] = {"str_template": "{v}-{w}"}
    with pytest.raises(SpecificationError) as excinfo:
        expand_parameterized_definitions(document)
    diagnostic = excinfo.value.diagnostics[0]
    assert diagnostic.condition == "for_str_template_collision"
    assert diagnostic.context["variables"] == ["v", "w"]


def test_str_template_collision_canonical_form_is_an_error():
    document = base_document()
    document["rows"][1]["derivations"]["LABEL"] = {
        "str_template": {"template": "week {w}", "missing": "UNKNOWN"}
    }
    with pytest.raises(SpecificationError) as excinfo:
        expand_parameterized_definitions(document)
    assert excinfo.value.diagnostics[0].condition == "for_str_template_collision"


def test_str_template_escapes_are_not_collisions_and_survive():
    document = base_document()
    document["rows"][1]["derivations"]["LABEL"] = {"str_template": "{{v}} week"}
    expanded = expand_parameterized_definitions(document)["rows"]
    assert expanded[1]["derivations"]["LABEL"] == {"str_template": "{{v}} week"}
    assert expanded[2]["derivations"]["LABEL"] == {"str_template": "{{v}} week"}


def test_escaped_braces_outside_templates_survive():
    document = base_document()
    document["intermediates"][0]["filter"] = "SRC.V = {v} AND SRC.NOTE = '{{v}}'"
    expanded = expand_parameterized_definitions(document)["intermediates"]
    assert expanded[0]["filter"] == "SRC.V = 1 AND SRC.NOTE = '{{v}}'"


def test_duplicate_expanded_ids_are_an_error():
    document = base_document()
    document["intermediates"][0]["for"] = [1, 1]
    with pytest.raises(SpecificationError) as excinfo:
        expand_parameterized_definitions(document)
    diagnostic = excinfo.value.diagnostics[0]
    assert diagnostic.condition == "duplicate_identifier"
    assert diagnostic.context["identifier"] == "PICK1"


def test_expanded_id_colliding_with_literal_is_an_error():
    document = base_document()
    document["intermediates"].append(
        {"id": "PICK1", "dataset": "SRC", "filter": "SRC.V = 1", "columns": ["X"]}
    )
    with pytest.raises(SpecificationError) as excinfo:
        expand_parameterized_definitions(document)
    assert excinfo.value.diagnostics[0].condition == "duplicate_identifier"


def test_placeholder_id_without_for_is_an_error():
    document = base_document()
    del document["intermediates"][0]["for"]
    with pytest.raises(SpecificationError) as excinfo:
        expand_parameterized_definitions(document)
    assert excinfo.value.diagnostics[0].condition == "placeholder_without_for"


def test_inherited_parameterized_definitions_expand_per_layer(tmp_path):
    from yamaa.schema.inheritance import resolve_specification

    parent = base_document()
    parent["rows"] = [parent["rows"][0]]
    write_spec(tmp_path, parent, "parent.yaml")
    child = {
        "schema_version": "1.0",
        "parents": "parent.yaml",
        "rows": [base_document()["rows"][1]],
    }
    entry = write_spec(tmp_path, child)
    resolved = resolve_specification(entry, load_schema_bundle(SCHEMA))
    assert [item["id"] for item in resolved.document["intermediates"]] == [
        "PICK1",
        "PICK2",
    ]
    assert [row["id"] for row in resolved.document["rows"]] == [
        "obs",
        "shifted1",
        "shifted2",
    ]
    origin = resolved.provenance["intermediates.PICK1.filter"]
    assert origin.file == (tmp_path / "parent.yaml").resolve()
