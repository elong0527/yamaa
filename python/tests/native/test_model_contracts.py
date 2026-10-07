"""Independent normalized model truth, additionally checked against the Python facade.

This is compile/component evidence. It never counts as shared original-YAML execution.
"""

import csv
import json
from pathlib import Path

import pytest
from pydantic import ValidationError

from yamaa.adapters._native_schema_wire import decode_nodes
from yamaa.specification.models import Specification

ROOT = Path(__file__).resolve().parents[3]
VECTORS = ROOT / "rust/crates/yamaa-adapters/tests/fixtures/schema_model.tsv"


def rows():
    with VECTORS.open(encoding="utf-8", newline="") as stream:
        return list(csv.DictReader(stream, delimiter="\t"))


def model_result(document):
    try:
        model = Specification.model_validate(document, strict=True)
    except ValidationError as error:
        return {
            "status": "invalid",
            "diagnostics": [
                {
                    "condition": "model_contract_mismatch",
                    "path": ".".join(str(member) for member in item["loc"]) or "$",
                    "requirement": None,
                    "context": [
                        {
                            "name": "reason",
                            "value": {"kind": "text", "value": item["msg"]},
                        }
                    ],
                }
                for item in error.errors(include_url=False, include_input=False)
            ],
        }
    return {"status": "model_valid", "default_driver": model.default_driver}


@pytest.mark.parametrize("row", rows(), ids=lambda row: row["id"])
def test_independent_truth_also_matches_reference_model(row):
    request, expected = json.loads(row["request"]), json.loads(row["expected"])
    for query, result in zip(
        request["queries"], expected["outcome"]["results"], strict=True
    ):
        tree = query["document"]
        assert model_result(decode_nodes(tree)[tree["root"]]) == result
