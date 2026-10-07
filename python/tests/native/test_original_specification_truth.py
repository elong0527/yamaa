"""Independent shared-run report truth also matches the original reference path."""

import json
import shutil
from pathlib import Path

import pytest

from yamaa.adapters.conformance import execute_example

ROOT = Path(__file__).resolve().parents[3]


@pytest.mark.parametrize(
    "name",
    [
        "negative-zero-division",
        "negative-integer-overflow",
        "adam-adlb-ordered-sum",
        "schema-lookup",
        "schema-window-functions",
        "schema-inheritance",
    ],
)
def test_authored_reports_match_reference_without_native_execution(name, tmp_path):
    actual = json.loads(
        execute_example(
            ROOT / "benchmarks" / name,
            schema_root=ROOT / "yaml",
            output_dir=tmp_path / name,
            backend="python",
        ).model_dump_json()
    )
    expected = json.loads(
        (
            ROOT / "rust/crates/yamaa-adapters/tests/fixtures/specs" / (name + ".json")
        ).read_text()
    )
    for field in ("runtime", "backend", "runtime_version", "engine_version"):
        expected[field] = actual[field]
    assert actual == expected


@pytest.mark.parametrize("minimum", [0, 18])
def test_reference_observer_retains_checks_before_late_declaration_failure(
    minimum, tmp_path
):
    """The failure log records completed checks, never unvisited declarations."""
    name = "adam-adlb-ordered-sum"
    case = tmp_path / name
    shutil.copytree(ROOT / "benchmarks" / name, case)
    specification = case / "spec.yaml"
    prefix = specification.read_text().split("verifications:")[0]
    specification.write_text(
        prefix
        + "verifications:\n"
        + f"  - row_count: {{id: count, min: {minimum}}}\n"
        + "  - row_count: {}\n"
        + "  - row_count: {min: 0}\n"
    )
    actual = json.loads(
        execute_example(
            case,
            schema_root=ROOT / "yaml",
            output_dir=tmp_path / "output",
            backend="python",
        ).model_dump_json()
    )
    expected = json.loads(
        (
            ROOT / "rust/crates/yamaa-adapters/tests/fixtures/specs" / (name + ".json")
        ).read_text()
    )
    for field in ("runtime", "backend", "runtime_version", "engine_version"):
        expected[field] = actual[field]
    expected["outcome"] = "failure"
    expected["artifacts"] = []
    expected["tables"].pop()
    expected["diagnostics"] = [
        {
            "phase": "validation",
            "condition": "invalid_declaration",
            "requirement": "REQ-0399",
            "spec_paths": ["verifications[1].row_count"],
            "context": {"reason": "row_count requires one bound"},
        }
    ]
    expected["nodes"][0]["outcome"] = "failure"
    expected["nodes"][0]["diagnostics"] = expected["diagnostics"]
    failure = None
    if minimum == 18:
        context = {
            "verification_id": "count",
            "count": 17,
            "failure_count": 1,
            "keys": [{}],
        }
        failure = {
            "phase": "verification",
            "condition": "row_count_failed",
            "requirement": "REQ-0385",
            "spec_paths": ["verifications[0].row_count"],
            "context": context,
            "severity": "error",
            "offending_keys": [{}],
            "log_context": context,
        }
    expected["verifications"] = [
        {
            "specification": "spec.yaml",
            "spec_path": "verifications[0].row_count",
            "check": "row_count",
            "target": None,
            "requirement": "REQ-0385",
            "verification_id": "count",
            "severity": "error",
            "evaluated_count": 1,
            "failure": failure,
        }
    ]
    assert actual == expected


@pytest.mark.parametrize("all_missing", [False, True])
def test_reference_sum_checks_values_after_missing_exclusion(all_missing, tmp_path):
    """Nonnumeric present values fail; an all-missing text group still reduces."""
    name = "adam-adlb-ordered-sum"
    case = tmp_path / name
    shutil.copytree(ROOT / "benchmarks" / name, case)
    specification = case / "spec.yaml"
    specification.write_text(
        specification.read_text().replace("LBSTRESN: float", "LBSTRESN: str")
    )
    source = case / "input/lb.csv"
    lines = source.read_text().splitlines()
    if all_missing:
        lines[1:] = [line.rsplit(",", 1)[0] + "," for line in lines[1:]]
        source.write_text("\n".join(lines) + "\n")
    actual = json.loads(
        execute_example(
            case,
            schema_root=ROOT / "yaml",
            output_dir=tmp_path / "output",
            backend="python",
        ).model_dump_json()
    )
    expected = json.loads(
        (
            ROOT / "rust/crates/yamaa-adapters/tests/fixtures/specs" / (name + ".json")
        ).read_text()
    )
    for field in ("runtime", "backend", "runtime_version", "engine_version"):
        expected[field] = actual[field]
    expected["tables"][0]["types"][6] = "str"
    for row, line in zip(expected["tables"][0]["rows"], lines[1:], strict=True):
        value = line.rsplit(",", 1)[1]
        row[6] = (
            {"type": "str", "value": value}
            if value
            else {"type": "missing", "value": None}
        )
    if all_missing:
        for row in expected["tables"][1]["rows"]:
            row[5] = {"type": "missing", "value": None}
        records = (case / "expected/adlb.csv").read_text().splitlines()
        for i in range(1, len(records)):
            cells = records[i].split(",")
            cells[5] = ""
            records[i] = ",".join(cells)
        content = "\n".join(records) + "\n"
        expected["artifacts"][0].update(
            records=records, content=content, byte_length=len(content.encode())
        )
        assert (tmp_path / "output/adlb.csv").read_bytes() == content.encode()
    else:
        expected["outcome"] = "failure"
        expected["artifacts"] = []
        expected["verifications"] = []
        expected["tables"].pop()
        expected["diagnostics"] = [
            {
                "phase": "validation",
                "condition": "incompatible_input_type",
                "requirement": "REQ-0510",
                "spec_paths": ["rows[1].derivations.AVAL.aggregate"],
                "context": {
                    "expr": "SUM(LB.LBSTRESN)",
                    "reducer": "SUM",
                    "source": "LB.LBSTRESN",
                    "expected": "numeric",
                    "actual": "str",
                },
            }
        ]
        expected["nodes"][0]["outcome"] = "failure"
        expected["nodes"][0]["diagnostics"] = expected["diagnostics"]
    assert actual == expected


@pytest.mark.parametrize(
    "variant",
    ["defaults", "unknown", "unknown_group", "unknown_sum", "coverage", "aggregate"],
)
def test_reference_row_default_scope_and_preflight_order(variant, tmp_path):
    """Independent original-document mutations pin inherited paths and coverage."""
    name = "adam-adlb-ordered-sum"
    case = tmp_path / name
    shutil.copytree(ROOT / "benchmarks" / name, case)
    specification = case / "spec.yaml"
    original = specification.read_text()
    raw = (
        original.replace(
            "label: Parameter\n", "label: Parameter\n    derivation: LB.LBTEST\n"
        )
        .replace("      PARAM: LB.LBTEST\n", "")
        .replace(
            "label: Derivation Type\n",
            "label: Derivation Type\n    derivation: {literal: null}\n",
        )
        .replace("      DTYPE: {literal: null}\n", "")
    )

    def diagnostic(condition, requirement, paths, context):
        return {
            "phase": "validation",
            "condition": condition,
            "requirement": requirement,
            "spec_paths": paths,
            "context": context,
        }

    expected = []
    if variant == "unknown":
        raw = raw.replace("derivation: LB.LBTEST", "derivation: LB.ABSENT")
        expected = [
            diagnostic(
                "unknown_field",
                "REQ-0103",
                ["columns.PARAM.derivation.source"],
                {"identifier": "LB.ABSENT"},
            )
        ]
    elif variant == "unknown_group":
        raw = original.replace(
            "group_by: [LB.STUDYID, LB.USUBJID, LB.VISIT]",
            "group_by: [LB.STUDYID, LB.USUBJID, LB.VISIT, LB.ABSENT]",
        )
        expected = [
            diagnostic(
                "unknown_field",
                "REQ-0103",
                ["rows[1].group_by[3]"],
                {"identifier": "LB.ABSENT"},
            )
        ]
    elif variant == "unknown_sum":
        raw = original.replace("SUM(LB.LBSTRESN)", "SUM(LB.ABSENT)")
        expected = [
            diagnostic(
                "unknown_field",
                "REQ-0103",
                ["rows[1].derivations.AVAL.aggregate"],
                {"identifier": "LB.ABSENT"},
            )
        ]
    elif variant == "coverage":
        raw = (
            original.replace("domain: ADLB", "domain: ADLB\nfilter: 'TRUE'")
            .replace("      PARAM: LB.LBTEST\n", "")
            .replace(
                "    derivations:\n",
                "    derivations:\n      ABSENT: {literal: ignored}\n",
                1,
            )
        )
        expected = [
            diagnostic(
                "undeclared_column",
                None,
                ["rows[0].derivations.ABSENT"],
                {"column": "ABSENT"},
            ),
            diagnostic(
                "missing_derivation",
                "REQ-0200",
                ["columns.PARAM.derivation"],
                {"column": "PARAM", "rows": ["collected"]},
            ),
            diagnostic(
                "conflicting_row_construction", "REQ-1171", ["filter", "rows"], {}
            ),
        ]
    elif variant == "aggregate":
        raw = original.replace(
            "label: Analysis Value\n",
            "label: Analysis Value\n    derivation: {aggregate: {expr: 'SUM(LB.LBSTRESN)'}}\n",
        )
        expected = [
            diagnostic(
                "duplicate_derivation",
                "REQ-1260",
                ["columns.AVAL.derivation"],
                {"column": "AVAL", "rows": ["collected", "total"]},
            )
        ]
    specification.write_text(raw)
    actual = json.loads(
        execute_example(
            case,
            schema_root=ROOT / "yaml",
            output_dir=tmp_path / "output",
            backend="python",
        ).model_dump_json()
    )
    assert actual["diagnostics"] == expected
    if variant == "defaults":
        truth = json.loads(
            (
                ROOT
                / "rust/crates/yamaa-adapters/tests/fixtures/specs"
                / (name + ".json")
            ).read_text()
        )
        for field in ("runtime", "backend", "runtime_version", "engine_version"):
            truth[field] = actual[field]
        assert actual == truth
        assert (tmp_path / "output/adlb.csv").read_bytes() == (
            case / "expected/adlb.csv"
        ).read_bytes()
    else:
        assert actual["outcome"] == "failure"
        assert not actual["artifacts"]
        assert not actual["verifications"]
        assert len(actual["source_reads"]) == (
            1 if variant in {"unknown", "unknown_group", "unknown_sum"} else 0
        )


@pytest.mark.parametrize(
    "expr,reason",
    [
        ("SUM(AVAL)", "a grouped row aggregate reads its row driver"),
        ("SUM(ABSENT)", "a grouped row aggregate reads its row driver"),
        ("SUM(OTHER.X)", "a grouped row aggregate reads 'LB', not 'OTHER'"),
    ],
)
@pytest.mark.parametrize("invalid_source", [False, True])
def test_reference_grouped_aggregate_scope_follows_ingestion(
    expr, reason, invalid_source, tmp_path
):
    case = tmp_path / "adam-adlb-ordered-sum"
    shutil.copytree(ROOT / "benchmarks/adam-adlb-ordered-sum", case)
    specification = case / "spec.yaml"
    specification.write_text(
        specification.read_text().replace("SUM(LB.LBSTRESN)", expr)
    )
    if invalid_source:
        source = case / "input/lb.csv"
        source.write_text(source.read_text().replace(",0.1", ",invalid", 1))
    actual = execute_example(
        case,
        schema_root=ROOT / "yaml",
        output_dir=tmp_path / "output",
        backend="python",
    ).model_dump(mode="json")
    expected = {
        "phase": "validation",
        "condition": "invalid_aggregate_context",
        "requirement": "REQ-0329",
        "spec_paths": ["rows[1].derivations.AVAL.aggregate"],
        "context": {"expr": expr, "reason": reason},
    }
    if invalid_source:
        expected = {
            "phase": "ingest",
            "condition": "field_parse_failed",
            "requirement": "REQ-0536",
            "spec_paths": ["input.LB.types.LBSTRESN"],
            "context": {
                "dataset": "LB",
                "field": "LBSTRESN",
                "type": "float",
                "value": "invalid",
            },
        }
    assert actual["outcome"] == "failure"
    assert actual["diagnostics"] == [expected]
    assert len(actual["source_reads"]) == 1
    assert len(actual["tables"]) == (0 if invalid_source else 1)
    assert not actual["artifacts"]
    assert not actual["verifications"]


def test_lookup_collects_ingestion_findings_in_source_order(tmp_path):
    case = tmp_path / "schema-lookup"
    shutil.copytree(ROOT / "benchmarks/schema-lookup", case)
    spec = case / "spec.yaml"
    spec.write_text(
        spec.read_text()
        .replace("DM: input/dm.csv", "DM: {path: input/dm.csv, types: {ABSENT: int}}")
        .replace("AE: input/ae.csv", "AE: {path: input/ae.csv, types: {AEDY: date}}")
    )
    actual = execute_example(
        case, schema_root=ROOT / "yaml", output_dir=tmp_path / "out", backend="python"
    ).model_dump(mode="json")
    assert actual["outcome"] == "failure"
    assert actual["diagnostics"] == [
        {
            "phase": "validation",
            "condition": "unknown_field",
            "requirement": "REQ-0532",
            "spec_paths": ["input.DM.types.ABSENT"],
            "context": {"dataset": "DM", "field": "ABSENT"},
        },
        {
            "phase": "ingest",
            "condition": "field_parse_failed",
            "requirement": "REQ-0536",
            "spec_paths": ["input.AE.types.AEDY"],
            "context": {
                "dataset": "AE",
                "field": "AEDY",
                "type": "date",
                "value": "50",
            },
        },
    ]
    assert [r["path"] for r in actual["source_reads"]] == [
        "input/dm.csv",
        "input/ae.csv",
        "input/meddict.csv",
    ]
    assert all(r["snapshots_created"] == 1 for r in actual["source_reads"])
    for field in ("artifacts", "tables", "verifications", "handler_counts"):
        assert actual[field] == []


LOOKUP_FAILURES = json.loads(
    (
        ROOT / "rust/crates/yamaa-adapters/tests/fixtures/specs/lookup-failures.json"
    ).read_text()
)


@pytest.mark.parametrize("variant", LOOKUP_FAILURES, ids=lambda case: case["name"])
def test_authored_lookup_failures_match_independent_reference(variant, tmp_path):
    case = tmp_path / "schema-lookup"
    shutil.copytree(ROOT / "benchmarks/schema-lookup", case)
    path = case / "spec.yaml"
    path.write_text(path.read_text().replace(variant["before"], variant["after"]))
    actual = execute_example(
        case,
        schema_root=ROOT / "yaml",
        output_dir=tmp_path / "output",
        backend="python",
    ).model_dump(mode="json")
    expected = json.loads(
        (
            ROOT / "rust/crates/yamaa-adapters/tests/fixtures/specs/schema-lookup.json"
        ).read_text()
    )
    for field in ("runtime", "backend", "runtime_version", "engine_version"):
        expected[field] = actual[field]
    expected["outcome"] = expected["nodes"][0]["outcome"] = "failure"
    for field in ("diagnostics", "handler_counts"):
        expected[field] = expected["nodes"][0][field] = variant[field]
    expected["artifacts"] = expected["verifications"] = []
    expected["tables"] = [
        table for table in expected["tables"] if table["stage"] == "source"
    ]
    assert actual == expected


WINDOW_FAILURES = json.loads(
    (
        ROOT / "rust/crates/yamaa-adapters/tests/fixtures/specs/window-failures.json"
    ).read_text()
)


@pytest.mark.parametrize("variant", WINDOW_FAILURES, ids=lambda case: case["name"])
def test_authored_window_failures_match_independent_reference(variant, tmp_path):
    case = tmp_path / "schema-window-functions"
    shutil.copytree(ROOT / "benchmarks/schema-window-functions", case)
    path = case / "spec.yaml"
    path.write_text(path.read_text().replace(variant["before"], variant["after"]))
    if "input_before" in variant:
        source = case / "input/vs.csv"
        source.write_text(
            source.read_text().replace(variant["input_before"], variant["input_after"])
        )
    actual = execute_example(
        case,
        schema_root=ROOT / "yaml",
        output_dir=tmp_path / "output",
        backend="python",
    ).model_dump(mode="json")
    expected = json.loads(
        (
            ROOT
            / "rust/crates/yamaa-adapters/tests/fixtures/specs/schema-window-functions.json"
        ).read_text()
    )
    for field in ("runtime", "backend", "runtime_version", "engine_version"):
        expected[field] = actual[field]
    expected["outcome"] = expected["nodes"][0]["outcome"] = "failure"
    for field in ("diagnostics", "handler_counts"):
        expected[field] = expected["nodes"][0][field] = variant[field]
    expected["artifacts"] = expected["verifications"] = []
    expected["tables"] = [
        table for table in expected["tables"] if table["stage"] == "source"
    ]
    for row, column, value in variant.get("source_cells", []):
        expected["tables"][0]["rows"][row][column] = value
    assert actual == expected
