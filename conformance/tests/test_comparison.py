from __future__ import annotations

import hashlib
import json
from collections.abc import Callable
from pathlib import Path

import pytest
from pydantic import ValidationError

import yaml
from conformance.__main__ import main
from conformance.comparison import compare_case
from conformance.models import Invocation, Report
from conformance.schema import rendered_schema


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _write_json(path: Path, value: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2) + "\n", encoding="utf-8")


def _positive_run(tmp_path: Path, *, temporal: bool = False) -> tuple[Path, Path]:
    project = tmp_path / "project"
    example = project / "yaml/examples/sdtm-dm-basic"
    expected = example / "expected"
    source = example / "input/source.csv"
    expected.mkdir(parents=True)
    source.parent.mkdir()

    if temporal:
        columns = [
            {"name": "DOMAIN", "type": "str"},
            {"name": "DATE", "type": "date"},
        ]
        output_columns = ["DOMAIN", "DATE"]
        csv = "DOMAIN,DATE\nDM,2026-09-13\nDM,\n"
        cells = [
            [
                {"missing": False, "value": "DM"},
                {
                    "missing": False,
                    "value": "2026-09-13",
                    "precision": "day",
                },
            ],
            [{"missing": False, "value": "DM"}, {"missing": True}],
        ]
    else:
        columns = [
            {"name": "DOMAIN", "type": "str"},
            {"name": "AGE", "type": "int"},
        ]
        output_columns = ["DOMAIN", "AGE"]
        csv = "DOMAIN,AGE\nDM,34\nDM,\n"
        cells = [
            [
                {"missing": False, "value": "DM"},
                {"missing": False, "value": "34"},
            ],
            [{"missing": False, "value": "DM"}, {"missing": True}],
        ]

    specification = {
        "schema_version": "1.0",
        "domain": "DM",
        "datasets": {"SOURCE": {"path": "input/source.csv"}},
        "keys": ["DOMAIN"],
        "output": {"path": "dm.csv", "columns": output_columns},
        "columns": columns,
    }
    spec = example / "spec.yaml"
    spec.write_text(yaml.safe_dump(specification, sort_keys=False), encoding="utf-8")
    source.write_text("id\n1\n", encoding="utf-8")
    (expected / "dm.csv").write_text(csv, encoding="utf-8")
    (expected / "handler-counts.yaml").write_text(
        'version: "1.0"\n'
        "specifications:\n"
        "  spec.yaml:\n"
        "    - spec_path: columns.DOMAIN.derivation.source.missing\n"
        "      handler: missing\n"
        "      count: 0\n"
        "    - spec_path: columns.AGE.derivation.source.missing\n"
        "      handler: missing\n"
        "      count: 1\n",
        encoding="utf-8",
    )

    run_root = tmp_path / "runs"
    entrypoint = "yaml/examples/sdtm-dm-basic/spec.yaml"
    for runtime in ("r", "python"):
        root = run_root / runtime
        artifact = root / "artifacts/primary.csv"
        resolved = root / "resolved-specification.json"
        artifact.parent.mkdir(parents=True)
        artifact.write_text(csv, encoding="utf-8")
        _write_json(resolved, specification)
        invocation = {
            "protocol_version": "1.0",
            "run_id": "case-001",
            "example": "sdtm-dm-basic",
            "runtime": runtime,
            "project_root": str(project),
            "schema_root": "yaml",
            "entrypoint": entrypoint,
            "data_roots": [],
            "output_directory": str(root),
        }
        invocation_path = root / "invocation.json"
        _write_json(invocation_path, invocation)
        report = {
            "protocol_version": "1.0",
            "invocation_sha256": _sha256(invocation_path),
            "example": "sdtm-dm-basic",
            "runtime": {
                "name": runtime,
                "language_version": "test",
                "implementation_version": "0.1.0",
            },
            "specification": {
                "entrypoint": entrypoint,
                "documents": [{"path": entrypoint, "sha256": _sha256(spec)}],
                "resolved": {
                    "path": "resolved-specification.json",
                    "sha256": _sha256(resolved),
                },
            },
            "sources": [
                {
                    "dataset": "SOURCE",
                    "declared_path": "input/source.csv",
                    "sha256": _sha256(source),
                }
            ],
            "handler_counts": [
                {
                    "spec_path": "columns.DOMAIN.derivation.source.missing",
                    "handler": "missing",
                    "count": 0,
                },
                {
                    "spec_path": "columns.AGE.derivation.source.missing",
                    "handler": "missing",
                    "count": 1,
                },
            ],
            "outcome": {
                "status": "success",
                "artifacts": [
                    {
                        "role": "primary",
                        "declared_path": "dm.csv",
                        "produced_path": "artifacts/primary.csv",
                        "profile": "csv",
                        "sha256": _sha256(artifact),
                        "table": {"columns": columns, "rows": cells},
                    }
                ],
                "diagnostics": [],
            },
        }
        _write_json(root / "report.json", report)
    return run_root, expected


def _negative_run(tmp_path: Path) -> tuple[Path, Path]:
    project = tmp_path / "project"
    example = project / "yaml/examples/negative-column-type-unknown"
    expected = example / "expected"
    expected.mkdir(parents=True)
    specification = {
        "schema_version": "1.0",
        "domain": "ADLB",
        "datasets": {"LB": {"path": "input/lb.csv"}},
        "keys": ["AVAL"],
        "output": {"path": "adlb.csv", "columns": ["AVAL"]},
        "columns": [{"name": "AVAL", "type": "number"}],
    }
    spec = example / "spec.yaml"
    spec.write_text(yaml.safe_dump(specification, sort_keys=False), encoding="utf-8")
    error = {
        "phase": "validation",
        "condition": "value_not_permitted",
        "spec_paths": ["columns.AVAL.type"],
        "requirement": "R011-29",
        "context": {
            "value": "number",
            "permitted": ["str", "int", "float", "date", "datetime"],
        },
    }
    (expected / "error.yaml").write_text(
        yaml.safe_dump(error, sort_keys=False), encoding="utf-8"
    )
    run_root = tmp_path / "runs"
    entrypoint = "yaml/examples/negative-column-type-unknown/spec.yaml"
    for runtime in ("r", "python"):
        root = run_root / runtime
        resolved = root / "resolved-specification.json"
        _write_json(resolved, specification)
        invocation = {
            "protocol_version": "1.0",
            "run_id": "case-002",
            "example": "negative-column-type-unknown",
            "runtime": runtime,
            "project_root": str(project),
            "schema_root": "yaml",
            "entrypoint": entrypoint,
            "data_roots": [],
            "output_directory": str(root),
        }
        invocation_path = root / "invocation.json"
        _write_json(invocation_path, invocation)
        report = {
            "protocol_version": "1.0",
            "invocation_sha256": _sha256(invocation_path),
            "example": "negative-column-type-unknown",
            "runtime": {
                "name": runtime,
                "language_version": "test",
                "implementation_version": "0.1.0",
            },
            "specification": {
                "entrypoint": entrypoint,
                "documents": [{"path": entrypoint, "sha256": _sha256(spec)}],
                "resolved": {
                    "path": "resolved-specification.json",
                    "sha256": _sha256(resolved),
                },
            },
            "sources": [],
            "handler_counts": [],
            "outcome": {
                "status": "failure",
                "diagnostics": [{"severity": "error", **error}],
            },
        }
        _write_json(root / "report.json", report)
    return run_root, expected


def _edit_json(path: Path, edit: Callable[[dict[str, object]], None]) -> None:
    value = json.loads(path.read_text(encoding="utf-8"))
    edit(value)
    _write_json(path, value)


def test_positive_pair_matches_golden_and_runtime_parity(tmp_path: Path) -> None:
    run_root, expected = _positive_run(tmp_path)

    summary = compare_case(run_root, expected)

    assert summary.status == "pass"
    assert summary.failures == ()


def test_cli_writes_concise_summary(
    tmp_path: Path, capsys: pytest.CaptureFixture[str]
) -> None:
    run_root, expected = _positive_run(tmp_path)
    summary_path = run_root / "summary.json"

    result = main(
        [
            "compare",
            "--run-root",
            str(run_root),
            "--expected-root",
            str(expected),
            "--runtime",
            "r",
            "--runtime",
            "python",
            "--summary",
            str(summary_path),
        ]
    )

    assert result == 0
    assert capsys.readouterr().out == "PASS: sdtm-dm-basic (r, python)\n"
    assert json.loads(summary_path.read_text(encoding="utf-8")) == {
        "protocol_version": "1.0",
        "example": "sdtm-dm-basic",
        "status": "pass",
        "runtimes": ["r", "python"],
        "checks": ["contract", "provenance", "golden", "runtime-parity"],
        "failures": [],
    }


@pytest.mark.parametrize(
    "mutate",
    [
        lambda text: text.replace("DM,34", "DM,35"),
        lambda text: text.replace("DOMAIN,AGE", "AGE,DOMAIN"),
        lambda text: text.replace("DM,34\nDM,", "DM,\nDM,34"),
        lambda text: text.replace("DM,\n", 'DM,""\n'),
    ],
    ids=("cell", "column-order", "row-order", "null-rendering"),
)
def test_golden_mutations_fail(tmp_path: Path, mutate: Callable[[str], str]) -> None:
    run_root, expected = _positive_run(tmp_path)
    golden = expected / "dm.csv"
    golden.write_text(mutate(golden.read_text(encoding="utf-8")), encoding="utf-8")

    summary = compare_case(run_root, expected)

    assert summary.status == "fail"
    assert any("artifact bytes differ" in failure for failure in summary.failures)


def test_runtime_type_mutation_fails_resolved_contract(tmp_path: Path) -> None:
    run_root, expected = _positive_run(tmp_path)
    for runtime in ("r", "python"):
        report = run_root / runtime / "report.json"

        def mutate(value: dict[str, object]) -> None:
            outcome = value["outcome"]
            assert isinstance(outcome, dict)
            artifacts = outcome["artifacts"]
            assert isinstance(artifacts, list)
            table = artifacts[0]["table"]
            table["columns"][1]["type"] = "str"

        _edit_json(report, mutate)

    summary = compare_case(run_root, expected)

    assert summary.status == "fail"
    assert any("runtime types" in failure for failure in summary.failures)


def test_typed_cell_mutation_fails_runtime_parity(tmp_path: Path) -> None:
    run_root, expected = _positive_run(tmp_path)
    report = run_root / "python/report.json"

    def mutate(value: dict[str, object]) -> None:
        value["outcome"]["artifacts"][0]["table"]["rows"][0][1]["value"] = "35"

    _edit_json(report, mutate)

    summary = compare_case(run_root, expected)

    assert summary.status == "fail"
    assert any("typed cells" in failure for failure in summary.failures)


def test_temporal_precision_mutation_fails_runtime_parity(tmp_path: Path) -> None:
    run_root, expected = _positive_run(tmp_path, temporal=True)
    report = run_root / "python/report.json"

    def mutate(value: dict[str, object]) -> None:
        value["outcome"]["artifacts"][0]["table"]["rows"][0][1]["precision"] = "month"

    _edit_json(report, mutate)

    summary = compare_case(run_root, expected)

    assert summary.status == "fail"
    assert any("temporal precision" in failure for failure in summary.failures)


def test_handler_count_mutation_fails(tmp_path: Path) -> None:
    run_root, expected = _positive_run(tmp_path)
    report = run_root / "python/report.json"

    def mutate(value: dict[str, object]) -> None:
        value["handler_counts"][0]["count"] = 2

    _edit_json(report, mutate)

    summary = compare_case(run_root, expected)

    assert summary.status == "fail"
    assert any("handler counts" in failure for failure in summary.failures)


@pytest.mark.parametrize(
    ("field", "replacement"),
    [
        ("phase", "derivation"),
        ("condition", "different_condition"),
        ("spec_paths", ["columns.OTHER.type"]),
        ("requirement", "R011-30"),
        ("context", {"value": "float"}),
    ],
)
def test_negative_contract_mutations_fail(
    tmp_path: Path, field: str, replacement: object
) -> None:
    run_root, expected = _negative_run(tmp_path)
    contract = expected / "error.yaml"
    value = yaml.safe_load(contract.read_text(encoding="utf-8"))
    value[field] = replacement
    contract.write_text(yaml.safe_dump(value, sort_keys=False), encoding="utf-8")

    summary = compare_case(run_root, expected)

    assert summary.status == "fail"
    assert any("diagnostic" in failure for failure in summary.failures)


@pytest.mark.parametrize(
    "outcome",
    [
        {
            "status": "unsupported",
            "features": [{"operation": "mapping", "spec_path": "columns.AVAL"}],
        },
        {"status": "blocked", "blockers": ["#215"]},
        {
            "status": "infrastructure_failure",
            "condition": "dependency_missing",
            "context": {"dependency": "engine"},
        },
    ],
    ids=("unsupported", "blocked", "infrastructure"),
)
def test_nonsemantic_outcome_cannot_satisfy_negative(
    tmp_path: Path, outcome: dict[str, object]
) -> None:
    run_root, expected = _negative_run(tmp_path)
    report = run_root / "python/report.json"
    _edit_json(report, lambda value: value.__setitem__("outcome", outcome))

    summary = compare_case(run_root, expected)

    assert summary.status == "fail"
    assert any("not 'failure'" in failure for failure in summary.failures)


def test_validation_failure_cannot_claim_source_consumption(tmp_path: Path) -> None:
    run_root, _ = _negative_run(tmp_path)
    report_path = run_root / "python/report.json"
    value = json.loads(report_path.read_text(encoding="utf-8"))
    value["sources"] = [
        {
            "dataset": "LB",
            "declared_path": "input/lb.csv",
            "sha256": "0" * 64,
        }
    ]

    with pytest.raises(ValidationError, match="cannot report consumed sources"):
        Report.model_validate_json(json.dumps(value), strict=True)


def test_invocation_forbids_expected_output_directory(tmp_path: Path) -> None:
    project = tmp_path / "project"
    with pytest.raises(ValidationError, match="inside expected"):
        Invocation.model_validate(
            {
                "protocol_version": "1.0",
                "run_id": "case",
                "example": "sdtm-dm-basic",
                "runtime": "python",
                "project_root": str(project),
                "schema_root": "yaml",
                "entrypoint": "yaml/examples/sdtm-dm-basic/spec.yaml",
                "data_roots": (),
                "output_directory": str(
                    project / "yaml/examples/sdtm-dm-basic/expected/python"
                ),
            },
            strict=True,
        )


def test_invocation_has_no_expected_contract_field(tmp_path: Path) -> None:
    run_root, _ = _positive_run(tmp_path)
    invocation = (run_root / "python/invocation.json").read_text(encoding="utf-8")

    assert "expected" not in invocation


def test_report_rejects_noncanonical_float_bits(tmp_path: Path) -> None:
    run_root, _ = _positive_run(tmp_path)
    report_path = run_root / "python/report.json"
    value = json.loads(report_path.read_text(encoding="utf-8"))
    table = value["outcome"]["artifacts"][0]["table"]
    table["columns"][1]["type"] = "float"
    table["rows"][0][1]["value"] = "34.0"

    with pytest.raises(ValidationError, match="IEEE-754"):
        Report.model_validate_json(json.dumps(value), strict=True)


def test_committed_schema_matches_models() -> None:
    schema_path = Path(__file__).parents[1] / "protocol.schema.json"

    assert schema_path.read_text(encoding="utf-8") == rendered_schema()
