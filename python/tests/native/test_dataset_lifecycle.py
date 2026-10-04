"""Reference truth for the typed Rust dataset plan's literal conversion timing."""

from pathlib import Path

import pytest
import yaml

from yamaa.io.polars import frame_from_values
from yamaa.models import TypedColumn
from yamaa.runtime.executor import (
    ExecutionFailure,
    ExecutionSuccess,
    execute_specification,
)
from yamaa.specification import load_specification


@pytest.mark.parametrize("literal", ["TOTAL", True])
@pytest.mark.parametrize("grouped", [False, True])
@pytest.mark.parametrize("count", [0, 1])
def test_literal_conversion_is_a_per_candidate_lifecycle(
    tmp_path, literal, grouped, count
):
    """A valid literal is converted only when its template actually builds a row."""
    row = {"id": "collected", "derivations": {"value": {"literal": literal}}}
    if grouped:
        row["group_by"] = ["T.id"]
    document = {
        "schema_version": "1.0",
        "domain": "TEST",
        "keys": ["id"],
        "input": {"T": {"path": "input.csv"}},
        "output": {"path": "output.csv", "columns": ["id", "value"]},
        "columns": [
            {"name": "id", "type": "str", "label": "ID", "derivation": "T.id"},
            {"name": "value", "type": "float", "label": "Value"},
        ],
        "rows": [row],
    }
    path = tmp_path / "spec.yaml"
    path.write_text(yaml.safe_dump(document), encoding="utf-8")
    schema = Path(__file__).parents[3] / "yaml"
    spec = load_specification(path, schema).specification
    table = frame_from_values((TypedColumn(name="id", type="str"),), [["a"]] * count)
    result = execute_specification(spec, {"T": table})
    if count == 0:
        assert isinstance(result, ExecutionSuccess)
        assert result.table.frame.height == 0
        assert result.handler_counts == ()
    else:
        assert isinstance(result, ExecutionFailure)
        assert result.handler_counts == ()
        assert [item.model_dump(mode="json") for item in result.diagnostics] == [
            {
                "phase": "convert",
                "condition": "conversion_failed",
                "spec_paths": ["columns.value"],
                "requirement": "REQ-0013",
                "context": {
                    "from": "bool" if isinstance(literal, bool) else "str",
                    "to": "float",
                    "value": literal,
                },
            }
        ]
