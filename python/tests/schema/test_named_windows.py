"""Complete named windows preserve inline behavior and caller scope."""

from __future__ import annotations

import copy
from pathlib import Path

import polars as pl
import pytest
import yaml
from polars.testing import assert_frame_equal

from yamaa import yamaa_domain
from yamaa.schema import resolve_specification
from yamaa.specification import SpecificationError, load_specification
from yamaa.specification._yaml import read_yaml_document
from yamaa.specification.schema import load_schema_bundle

ROOT = Path(__file__).parents[3]
SCHEMA = ROOT / "yaml"


def document():
    return {
        "schema_version": "1.0",
        "domain": "OUT",
        "input": {"SRC": {"path": "input.csv", "types": {"SEQ": "int", "VAL": "int"}}},
        "base": "SRC",
        "keys": ["ID"],
        "output": {"path": "out.csv", "columns": ["ID", "PREV"]},
        "windows": {"VISITS": {"group_by": ["G"], "order_by": ["SEQ"]}},
        "columns": [
            {"name": name, "type": kind, "label": name, "derivation": f"SRC.{name}"}
            for name, kind in [
                ("ID", "str"),
                ("G", "str"),
                ("SEQ", "int"),
                ("VAL", "int"),
            ]
        ]
        + [
            {
                "name": "PREV",
                "type": "int",
                "label": "Previous value",
                "derivation": {
                    "row_value": {"source": "VAL", "offset": -1, "window": "VISITS"}
                },
            }
        ],
    }


def write_spec(tmp_path, value, name="spec.yaml"):
    path = tmp_path / name
    path.write_text(yaml.safe_dump(value, sort_keys=False), encoding="ascii")
    (tmp_path / "input.csv").write_text(
        "ID,G,SEQ,VAL\n01,a,1,10\n02,a,2,20\n03,b,1,30\n04,b,2,40\n",
        encoding="ascii",
    )
    return path


def resolve(path):
    return resolve_specification(path, load_schema_bundle(SCHEMA))


@pytest.mark.parametrize(
    "benchmark",
    [
        "adam-adae-severity-rank",
        "adam-adrs-confirmed-response",
        "schema-window-functions",
    ],
)
def test_named_windows_equal_manual_inline_specification_and_golden(benchmark):
    directory = ROOT / "benchmarks" / benchmark
    entry = directory / "spec.yaml"
    authored = read_yaml_document(entry)
    inline = copy.deepcopy(authored)
    definitions = inline.pop("windows")
    for column in inline["columns"]:
        derivation = column.get("derivation")
        if isinstance(derivation, dict):
            for payload in derivation.values():
                if isinstance(payload, dict) and isinstance(payload.get("window"), str):
                    payload["window"] = copy.deepcopy(definitions[payload["window"]])
    expected = resolve_specification(
        entry, load_schema_bundle(SCHEMA), entry_document=inline
    ).document
    resolved = resolve(entry)
    assert resolved.document == expected
    assert load_specification(entry, SCHEMA).specification == resolved.specification
    run = yamaa_domain(entry, schema_root=SCHEMA)
    assert run.issues.is_empty()
    assert run.output is not None
    golden = directory / "expected" / Path(authored["output"]["path"]).name
    assert_frame_equal(
        run.output, pl.read_csv(golden, schema=run.output.schema), check_exact=True
    )


def test_inherited_definitions_replace_whole_and_keep_other_names(tmp_path):
    parent = document()
    parent["windows"]["UNUSED"] = {"group_by": ["NOT_A_COLUMN"]}
    write_spec(tmp_path, parent, "parent.yaml")
    child = {
        "schema_version": "1.0",
        "parents": "parent.yaml",
        "windows": {"VISITS": {"order_by": [{"variable": "SEQ", "direction": "desc"}]}},
    }
    resolved = resolve(write_spec(tmp_path, child))
    previous = next(
        col for col in resolved.document["columns"] if col["name"] == "PREV"
    )
    window = previous["derivation"]["value"]["row_value"]["window"]
    assert "group_by" not in window  # Whole replacement, not field merging.
    assert "windows" not in resolved.document
    assert "G" not in [col["name"] for col in resolved.document["columns"]]
    assert {"SEQ", "VAL"} <= {col["name"] for col in resolved.document["columns"]}
    reference = "columns.PREV.derivation.value.row_value.window"
    assert resolved.provenance[reference].file == (tmp_path / "parent.yaml").resolve()
    assert (
        resolved.provenance[reference + ".order_by"].file
        == (tmp_path / "spec.yaml").resolve()
    )
    assert (
        resolved.provenance[reference + ".order_by"].spec_path
        == "windows.VISITS.order_by"
    )


def test_inherited_window_dependencies_are_retained_and_ordered(tmp_path):
    parent = document()
    # The shared filter is also a dependency, even though it is never published.
    parent["windows"]["VISITS"]["filter"] = "VAL > 10"
    parent["columns"] = parent["columns"][-1:] + parent["columns"][:-1]
    write_spec(tmp_path, parent, "parent.yaml")
    child = {
        "schema_version": "1.0",
        "parents": "parent.yaml",
        "windows": {"ANOTHER": {"group_by": ["G"]}},
    }
    resolved = resolve(write_spec(tmp_path, child))
    names = [col["name"] for col in resolved.document["columns"]]
    assert {"G", "SEQ", "VAL"} <= set(names)
    assert all(names.index(name) < names.index("PREV") for name in ("G", "SEQ", "VAL"))
    assert (
        resolved.document["columns"][-1]["derivation"]["value"]["row_value"]["window"][
            "filter"
        ]
        == "VAL > 10"
    )


def test_unknown_reference_in_dead_inherited_column_is_pruned(tmp_path):
    parent = document()
    parent["columns"][-1]["derivation"]["row_value"]["window"] = "MISSING"
    write_spec(tmp_path, parent, "parent.yaml")
    child = {
        "schema_version": "1.0",
        "parents": "parent.yaml",
        "output": {"path": "out.csv", "columns": ["ID"]},
    }
    assert [
        col.name for col in resolve(write_spec(tmp_path, child)).specification.columns
    ] == ["ID"]


@pytest.mark.parametrize("definitions", [None, {}])
def test_unknown_reference_reports_use_site(tmp_path, definitions):
    value = document()
    if definitions is None:
        value.pop("windows")
    else:
        value["windows"] = definitions
    with pytest.raises(SpecificationError) as error:
        load_specification(write_spec(tmp_path, value), SCHEMA)
    diagnostic = error.value.diagnostics[0]
    assert diagnostic.condition == "unknown_window"
    assert diagnostic.spec_paths == ("columns[4].derivation.row_value.window",)
    assert diagnostic.context == {"window": "VISITS"}


@pytest.mark.parametrize(
    "window",
    [
        {"ref": "VISITS"},
        {"ref": "VISITS", "group_by": ["G"]},
        {"ref": "VISITS", "order_by": ["VAL"]},
    ],
)
def test_reference_mapping_and_overrides_are_rejected(tmp_path, window):
    value = document()
    value["columns"][-1]["derivation"]["row_value"]["window"] = window
    with pytest.raises(SpecificationError):
        load_specification(write_spec(tmp_path, value), SCHEMA)


@pytest.mark.parametrize(
    "definition", ["VISITS", {"ref": "VISITS"}, {"typo": ["ID"]}, None]
)
def test_unused_malformed_definitions_are_rejected(tmp_path, definition):
    value = document()
    value["windows"]["UNUSED"] = definition
    with pytest.raises(SpecificationError):
        load_specification(write_spec(tmp_path, value), SCHEMA)


def test_shared_window_is_evaluated_separately_in_each_row_template(tmp_path):
    value = document()
    value["windows"]["VISITS"] = {"order_by": ["SEQ"]}  # One partition per caller.
    value["output"]["columns"] = ["ID", "PREV"]
    value["rows"] = []
    derivations = {col["name"]: col.pop("derivation") for col in value["columns"]}
    for group in ("a", "b"):
        value["rows"].append(
            {
                "id": group,
                "dataset": "SRC",
                "filter": f"SRC.G = '{group}'",
                "derivations": copy.deepcopy(derivations),
            }
        )
    run = yamaa_domain(write_spec(tmp_path, value), schema_root=SCHEMA)
    assert run.issues.is_empty()
    assert run.output.to_dict(as_series=False) == {
        "ID": ["01", "02", "03", "04"],
        "PREV": [None, 10, None, 30],
    }


@pytest.mark.parametrize(
    "operation,window,condition",
    [
        ("row_number", {}, "window_order_by_required"),
        ("baseline_flag", {"order_by": ["SEQ"]}, "window_order_by_forbidden"),
    ],
)
def test_named_windows_preserve_operation_restrictions(
    tmp_path, operation, window, condition
):
    value = document()
    value["windows"]["VISITS"] = window
    payload = {"window": "VISITS"}
    if operation == "baseline_flag":
        payload.update(date="SEQ", reference_date="SEQ")
        value["columns"][-1]["type"] = "str"
    value["columns"][-1]["derivation"] = {operation: payload}
    run = yamaa_domain(write_spec(tmp_path, value), schema_root=SCHEMA)
    assert condition in run.issues["condition"].to_list()


@pytest.mark.parametrize("replacement", ["OTHER", {"order_by": ["VAL"]}])
def test_replacing_reference_with_inline_or_another_reference_is_atomic(
    tmp_path, replacement
):
    parent = document()
    parent["windows"]["OTHER"] = {"order_by": ["VAL"]}
    write_spec(tmp_path, parent, "parent.yaml")
    child = {
        "schema_version": "1.0",
        "parents": "parent.yaml",
        "columns": [
            {"name": "PREV", "derivation": {"row_value": {"window": replacement}}}
        ],
    }
    resolved = resolve(write_spec(tmp_path, child))
    previous = next(
        col for col in resolved.document["columns"] if col["name"] == "PREV"
    )
    window = previous["derivation"]["value"]["row_value"]["window"]
    assert "group_by" not in window
    assert window["order_by"][0]["variable"] == "VAL"


def test_clearing_collection_rejects_surviving_reference(tmp_path):
    write_spec(tmp_path, document(), "parent.yaml")
    child = {"schema_version": "1.0", "parents": "parent.yaml", "windows": None}
    with pytest.raises(SpecificationError) as error:
        resolve(write_spec(tmp_path, child))
    assert error.value.diagnostics[0].condition == "unknown_window"


def test_window_names_do_not_rewrite_metadata_or_share_mutable_settings(tmp_path):
    value = document()
    value["metadata"] = {"window": "UNDECLARED"}
    second = copy.deepcopy(value["columns"][-1])
    second["name"] = "PREV2"
    value["columns"].append(second)
    resolved = resolve(write_spec(tmp_path, value)).document
    first = resolved["columns"][-2]["derivation"]["value"]["row_value"]["window"]
    other = resolved["columns"][-1]["derivation"]["value"]["row_value"]["window"]
    first["group_by"].append("ID")
    assert other["group_by"] == ["G"]
    assert resolved["metadata"] == value["metadata"]
