"""Both public issue views are projections of the same native rows."""

import dataclasses

import polars as pl
import pytest

from yamaa._native_results import (
    ISSUE_SCHEMA,
    Diagnostic,
    SourceSpan,
    diagnostic_records,
    issues_frame,
)

# Unregistered probe conditions: the mapping under test is a pure projection.
ROWS = [
    (
        "validation",
        "probe_width",
        None,
        ["columns.N.derivation.literal"],
        '{"actual":18446744073709551616,"minimum":-9223372036854775808}',
        {"actual": 18446744073709551616, "minimum": -9223372036854775808},
        None,
        None,
    ),
    (
        "derivation",
        "probe_root",
        "REQ-0000",
        ["columns.R.derivation.compute"],
        '{"expr":"A / B","values":[null,1.5,"text",true,{}]}',
        {"expr": "A / B", "values": [None, 1.5, "text", True, {}]},
        (0, 0),
        [],
    ),
]


def test_frame_projects_the_first_five_fields_with_the_fixed_schema():
    frame = issues_frame(ROWS)
    assert frame.schema == pl.Schema(ISSUE_SCHEMA)
    assert frame.rows() == [row[:5] for row in ROWS]
    empty = issues_frame([])
    assert empty.schema == pl.Schema(ISSUE_SCHEMA)
    assert empty.height == 0


def test_records_carry_structured_context_and_distinguish_absent_geometry():
    records = diagnostic_records(ROWS)
    assert records == [
        Diagnostic(
            phase="validation",
            condition="probe_width",
            requirement=None,
            spec_paths=["columns.N.derivation.literal"],
            context={"actual": 18446744073709551616, "minimum": -(2**63)},
            source_span=None,
            operand_route=None,
        ),
        Diagnostic(
            phase="derivation",
            condition="probe_root",
            requirement="REQ-0000",
            spec_paths=["columns.R.derivation.compute"],
            context={"expr": "A / B", "values": [None, 1.5, "text", True, {}]},
            source_span=SourceSpan(0, 0),
            operand_route=[],
        ),
    ]
    assert records[0].source_span is None and records[1].source_span is not None
    assert records[0].operand_route is None and records[1].operand_route == []
    assert isinstance(records[0].context["actual"], int)
    assert isinstance(records[1].context["values"][1], float)
    assert diagnostic_records([]) == []


def test_records_are_frozen_and_agree_with_the_frame():
    frame = issues_frame(ROWS)
    for record, row in zip(diagnostic_records(ROWS), frame.to_dicts(), strict=True):
        assert (record.phase, record.condition, record.requirement) == (
            row["phase"],
            row["condition"],
            row["requirement"],
        )
        assert record.spec_paths == row["spec_paths"]
        with pytest.raises(dataclasses.FrozenInstanceError):
            record.phase = "changed"
