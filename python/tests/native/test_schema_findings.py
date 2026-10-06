"""Native admission diagnostics preserve authored registry and field locations."""

from pathlib import Path

import pytest

from yamaa.adapters._native_schema_findings import admission_error


@pytest.mark.parametrize(
    "path,issue,reason",
    [
        ("includes", {"code": "includes_list"}, "includes must be a list"),
        (
            "ops",
            {"code": "duplicate_registry_entry", "name": "one"},
            "duplicate registry entry ops.one",
        ),
        (
            "record",
            {"code": "field_entry"},
            "record: class fields must be one-entry mappings",
        ),
        ("$", {"code": "expected_mapping"}, "schema document must be a mapping"),
    ],
)
def test_admission_reason_keeps_each_finding_location_once(path, issue, reason):
    outcome = {
        "status": "invalid_schema",
        "issues": [{"module": 0, "path": path, "issue": issue}],
    }
    error = admission_error(outcome, Path("schema.yaml"), [[]], ["schema.yaml"])
    assert error.diagnostics[0].model_dump() == {
        "phase": "validation",
        "condition": "invalid_schema_bundle",
        "spec_paths": ("$",),
        "requirement": None,
        "context": {"path": "schema.yaml", "reason": reason},
    }
    assert error.native_outcome is outcome


@pytest.mark.parametrize(
    "origins,filename,reason",
    [
        ([1], "schema_other.yaml", "ops: registry is empty"),
        (
            [1, 1],
            "schema_other.yaml",
            "ops: registry is empty; ops: registry is empty",
        ),
        (
            [0, 1],
            "schema.yaml",
            "ops: registry is empty; ops: registry is empty",
        ),
    ],
)
def test_admission_origin_uses_one_module_or_the_cross_module_bundle(
    origins, filename, reason
):
    outcome = {
        "status": "invalid_schema",
        "issues": [
            {"module": origin, "path": "ops", "issue": {"code": "empty_registry"}}
            for origin in origins
        ],
    }
    error = admission_error(
        outcome,
        Path("schemas/schema.yaml"),
        [[], []],
        ["schema.yaml", "schema_other.yaml"],
    )
    assert error.diagnostics[0].context == {
        "path": str(Path("schemas") / filename),
        "reason": reason,
    }
    assert error.native_outcome is outcome
