"""REQ-0632 rejects clearing a value that the new declaration never inherited."""

import json
from pathlib import Path

import pytest

from yamaa.schema import resolve_specification
from yamaa.schema.inheritance import _merge_layers, _validate_layer
from yamaa.specification import SpecificationError
from yamaa.specification.schema import load_schema_bundle

SCHEMA_ROOT = Path(__file__).parents[3] / "yaml"


@pytest.mark.parametrize(
    "collection, members, logical",
    [
        ("columns", [{"name": "X", "type": "str", "label": None}], "columns.X.label"),
        ("input", {"DS": {"path": "data.csv", "types": None}}, "input.DS.types"),
        ("rows", [{"id": "R", "filter": None}], "rows.R.filter"),
        (
            "intermediates",
            [{"id": "I", "filter": None}],
            "intermediates.I.filter",
        ),
    ],
)
def test_new_member_cannot_clear_an_optional_field_without_an_inherited_value(
    collection, members, logical, tmp_path
):
    bundle = load_schema_bundle(SCHEMA_ROOT)
    layer, diagnostics = _validate_layer(
        {"schema_version": "1.0", collection: members}, bundle
    )
    assert diagnostics == []
    _, provenance, diagnostics = _merge_layers(
        [(tmp_path / "entry.yaml", layer)], bundle
    )
    assert [
        (item.condition, item.spec_paths, item.requirement) for item in diagnostics
    ] == [("invalid_clear", (logical,), "REQ-0660")]
    assert logical not in provenance


def test_standalone_null_handler_is_a_literal_until_inheritance_is_requested(tmp_path):
    document = {
        "schema_version": "1.0",
        "domain": "OUT",
        "keys": ["ID"],
        "base": "SRC",
        "input": {"SRC": "input.csv"},
        "output": {"path": "out.csv", "columns": ["ID"]},
        "columns": [{"name": "ID", "type": "str", "derivation": "SRC.ID"}],
        "intermediates": [
            {"id": "LOOK", "dataset": "SRC", "key": ["ID"], "no_match": None}
        ],
    }
    path = tmp_path / "spec.yaml"
    path.write_text(json.dumps(document), encoding="ascii")
    bundle = load_schema_bundle(SCHEMA_ROOT)
    resolved = resolve_specification(path, bundle)
    member = resolved.document["intermediates"][0]
    assert "no_match" in member and member["no_match"] is None
    assert resolved.provenance["intermediates.LOOK.no_match"].file == path.resolve()

    document["parents"] = []
    path.write_text(json.dumps(document), encoding="ascii")
    with pytest.raises(SpecificationError) as caught:
        resolve_specification(path, bundle)
    assert [(d.condition, d.spec_paths) for d in caught.value.diagnostics] == [
        ("invalid_clear", ("intermediates.LOOK.no_match",))
    ]
