"""Coverage gates must fail closed without confusing unsupported with parity."""

import json
import shutil
from pathlib import Path

import pytest

from yamaa import __version__
from yamaa.adapters.conformance import (
    ComparisonFinding,
    execute_example,
    read_report,
    write_report,
)
from yamaa.adapters.qualification import (
    Batch,
    KnownGap,
    MissingRoute,
    load_inventory,
    main,
    qualify,
)

ROOT = Path(__file__).parents[3]


@pytest.fixture
def suite(tmp_path):
    examples = tmp_path / "benchmarks"
    names = ("schema-lookup", "negative-zero-division")
    for name in names:
        shutil.copytree(ROOT / "benchmarks" / name, examples / name)
    (examples / "execution-manifest.yaml").write_text(
        'version: "1.0"\nexamples:\n'
        + "".join(
            f"  {name}: {{status: executable, runtimes: [python]}}\n" for name in names
        )
    )
    reference_dir = tmp_path / "reference"
    for name in names:
        report = execute_example(
            examples / name,
            schema_root=ROOT / "yaml",
            output_dir=tmp_path / "out" / name,
        )
        write_report(report, reference_dir)
    reference = Batch(
        runtime="python",
        backend="python",
        level="reference_run",
        source_revision="revision-a",
        host_package_version=__version__,
        artifact_reference="installed-wheel/yamaa.whl",
        evidence="local/reference-run",
        reports_dir=str(reference_dir),
    )
    return examples, reference


def native_copy(suite, tmp_path):
    _examples, reference = suite
    directory = tmp_path / "native"
    for path in Path(reference.reports_dir).glob("*.json"):
        # Synthetic candidate evidence tests the reporter, never claims native execution.
        write_report(
            read_report(path).model_copy(
                update={"backend": "rust", "engine_version": "0.1.0"}
            ),
            directory,
        )
    batch = Batch(
        runtime="python",
        backend="rust",
        level="reference_assisted_run",
        source_revision="revision-a",
        host_package_version=__version__,
        core_version="0.1.0",
        artifact_reference="native.whl",
        evidence="synthetic/reporter-test",
        reports_dir=str(directory),
    )
    return batch


def rows(inventory, runtime, backend):
    return [
        r for r in inventory.coverage if (r.runtime, r.backend) == (runtime, backend)
    ]


def test_complete_inventory_retains_unexercised_routes(suite):
    examples, reference = suite
    result = qualify(examples, "revision-a", (reference,))
    assert not result.errors
    assert len(result.coverage) == 6
    assert {r.result for r in rows(result, "python", "python")} == {"pass"}
    assert {r.result for r in rows(result, "python", "rust")} == {"not_exercised"}
    assert {r.blockers for r in rows(result, "r", "rust")} == {("report_not_supplied",)}
    assert result.manifest.examples["schema-lookup"].runtimes == ["python"]


def test_native_requires_golden_and_portable_parity_and_preserves_level(
    suite, tmp_path
):
    examples, reference = suite
    native = native_copy(suite, tmp_path)
    required = (("schema-lookup", "python", "rust", "shared_run"),)
    result = qualify(examples, "revision-a", (reference, native), required=required)
    assert {r.result for r in rows(result, "python", "rust")} == {"pass"}
    assert any("required qualification" in e for e in result.errors)
    assert all(
        r.level == "reference_assisted_run" for r in rows(result, "python", "rust")
    )


@pytest.mark.parametrize(
    "mutation", ["cell", "type", "order", "diagnostic", "handler", "table"]
)
def test_native_mutations_cannot_qualify(suite, tmp_path, mutation):
    examples, reference = suite
    native = native_copy(suite, tmp_path)
    name = "negative-zero-division" if mutation == "diagnostic" else "schema-lookup"
    path = Path(native.reports_dir) / f"{name}.python.rust.json"
    payload = json.loads(path.read_text())
    if mutation == "cell":
        payload["artifacts"][0]["content"] += "wrong\n"
    elif mutation == "type":
        payload["tables"][0]["types"][0] = "float"
    elif mutation == "order":
        payload["tables"][0]["rows"].reverse()
    elif mutation == "diagnostic":
        payload["diagnostics"][0]["condition"] = "wrong_condition"
    elif mutation == "handler":
        payload["handler_counts"].append(
            {"spec_path": "columns[0]", "handler": "unconvertible", "count": 1}
        )
    elif mutation == "table":
        payload["tables"].pop()
    path.write_text(json.dumps(payload))
    result = qualify(examples, "revision-a", (reference, native))
    row = next(r for r in rows(result, "python", "rust") if r.example == name)
    assert row.result == "semantic_mismatch"
    assert row.findings and result.errors


def test_unsupported_is_not_a_negative_pass_and_does_not_fail_unqualified_scope(
    suite, tmp_path
):
    examples, reference = suite
    native = native_copy(suite, tmp_path)
    path = Path(native.reports_dir) / "negative-zero-division.python.rust.json"
    report = read_report(path).model_copy(
        update={"outcome": "unsupported", "diagnostics": ()}
    )
    write_report(report, Path(native.reports_dir))
    result = qualify(examples, "revision-a", (reference, native))
    assert not result.errors
    assert (
        next(
            r for r in rows(result, "python", "rust") if r.example == report.example
        ).result
        == "unsupported"
    )
    gated = qualify(
        examples,
        "revision-a",
        (reference, native),
        required=((report.example, "python", "rust", "reference_assisted_run"),),
    )
    assert gated.errors


def test_missing_reference_cannot_qualify_native(suite, tmp_path):
    examples, _ = suite
    native = native_copy(suite, tmp_path)
    result = qualify(examples, "revision-a", (native,))
    assert {r.result for r in rows(result, "python", "rust")} == {
        "infrastructure_failure"
    }
    assert result.errors


@pytest.mark.parametrize(
    "mutation", ["malformed", "wrong_host", "wrong_version", "unknown_fixture"]
)
def test_invalid_reports_fail_inventory(suite, mutation):
    examples, reference = suite
    path = Path(reference.reports_dir) / "schema-lookup.python.python.json"
    payload = json.loads(path.read_text())
    if mutation == "malformed":
        payload.pop("tables")
    elif mutation == "wrong_host":
        payload["runtime"] = "r"
    elif mutation == "wrong_version":
        payload["engine_version"] = "stale"
    else:
        payload["example"] = "retired-case"
    path.write_text(json.dumps(payload))
    result = qualify(examples, "revision-a", (reference,))
    assert result.errors
    assert (
        next(
            r for r in rows(result, "python", "python") if r.example == "schema-lookup"
        ).result
        == "infrastructure_failure"
    )


@pytest.mark.parametrize("change", ["added", "removed", "duplicate"])
def test_inventory_reconciliation(suite, change):
    examples, _ = suite
    if change == "added":
        (examples / "new-case").mkdir()
        (examples / "new-case" / "spec.yaml").write_text("")
    elif change == "removed":
        shutil.rmtree(examples / "schema-lookup")
    else:
        with (examples / "execution-manifest.yaml").open("a") as handle:
            handle.write("  schema-lookup: {status: executable, runtimes: [python]}\n")
    with pytest.raises(ValueError):
        load_inventory(examples)


def test_stale_revision_or_duplicate_batch_is_rejected(suite):
    examples, reference = suite
    with pytest.raises(ValueError, match="stale batch"):
        qualify(examples, "revision-b", (reference,))
    with pytest.raises(ValueError, match="duplicate batch"):
        qualify(examples, "revision-a", (reference, reference))


def test_required_fixture_disappearance_is_visible(suite):
    examples, reference = suite
    result = qualify(
        examples,
        "revision-a",
        (reference,),
        required=(("retired-case", "python", "rust", "shared_run"),),
    )
    assert any("retired-case" in e for e in result.errors)


def test_cli_writes_deterministic_evidence_without_modifying_manifest(suite, tmp_path):
    examples, reference = suite
    batch = tmp_path / "batch.json"
    batch.write_text(reference.model_dump_json())
    output = tmp_path / "coverage.json"
    before = (examples / "execution-manifest.yaml").read_bytes()
    argv = [
        "--examples-root",
        str(examples),
        "--source-revision",
        "revision-a",
        "--batch",
        str(batch),
        "--output",
        str(output),
    ]
    assert main(argv) == 0
    first = output.read_bytes()
    assert main(argv) == 0
    assert output.read_bytes() == first
    assert (examples / "execution-manifest.yaml").read_bytes() == before


def test_missing_report_directory_is_an_infrastructure_failure(suite, tmp_path):
    examples, reference = suite
    native = native_copy(suite, tmp_path)
    shutil.rmtree(native.reports_dir)
    inventory = qualify(examples, "revision-a", (reference, native))
    assert {r.result for r in rows(inventory, "python", "rust")} == {
        "infrastructure_failure"
    }
    assert inventory.errors


def test_broken_expected_contract_is_an_infrastructure_failure(suite):
    examples, reference = suite
    (examples / "negative-zero-division" / "expected" / "error.yaml").write_text(
        "- not-a-mapping\n"
    )
    inventory = qualify(examples, "revision-a", (reference,))
    row = next(
        r
        for r in rows(inventory, "python", "python")
        if r.example == "negative-zero-division"
    )
    assert row.result == "infrastructure_failure"
    assert inventory.errors


def test_cli_enforces_the_committed_qualification_level(suite, tmp_path):
    examples, reference = suite
    native = native_copy(suite, tmp_path)
    arguments = ["--examples-root", str(examples), "--source-revision", "revision-a"]
    for name, batch in [("reference", reference), ("native", native)]:
        path = tmp_path / f"{name}-batch.json"
        path.write_text(batch.model_dump_json())
        arguments.extend(["--batch", str(path)])
    gates = tmp_path / "required.json"
    gates.write_text(json.dumps([["schema-lookup", "python", "rust", "shared_run"]]))
    arguments.extend(
        ["--required", str(gates), "--output", str(tmp_path / "inventory.json")]
    )
    assert main(arguments) == 1
    assert "required qualification" in (tmp_path / "inventory.json").read_text()


@pytest.mark.parametrize("backend", ["python", "rust"])
def test_diagnostic_free_failure_is_retained_as_broken_evidence(
    suite, tmp_path, backend
):
    """A malformed negative report fails without preventing a complete inventory."""
    examples, reference = suite
    batches = [reference]
    if backend == "rust":
        batches.append(native_copy(suite, tmp_path))
    directory = Path(batches[-1].reports_dir)
    report_path = directory / f"negative-zero-division.python.{backend}.json"
    payload = json.loads(report_path.read_text())
    payload["diagnostics"] = []
    report_path.write_text(json.dumps(payload))
    arguments = ["--examples-root", str(examples), "--source-revision", "revision-a"]
    for index, batch in enumerate(batches):
        path = tmp_path / f"batch-{index}.json"
        path.write_text(batch.model_dump_json())
        arguments.extend(["--batch", str(path)])
    output = tmp_path / "inventory.json"
    arguments.extend(["--output", str(output)])
    assert main(arguments) == 1
    inventory = json.loads(output.read_text())
    assert len(inventory["coverage"]) == 6
    row = next(
        row
        for row in inventory["coverage"]
        if (
            row["example"] == "negative-zero-division"
            and row["runtime"] == "python"
            and row["backend"] == backend
        )
    )
    assert row["result"] == "infrastructure_failure"
    assert "requires at least one diagnostic" in row["error"]


@pytest.fixture
def known_gap(suite, tmp_path):
    """An independently stated table-observation mismatch in synthetic evidence."""
    native = native_copy(suite, tmp_path)
    path = Path(native.reports_dir) / "schema-lookup.python.rust.json"
    payload = json.loads(path.read_text())
    assert len(payload["tables"]) == 4  # three sources and the derived output
    payload["tables"] = []
    path.write_text(json.dumps(payload))
    gap = KnownGap(
        example="schema-lookup",
        runtime="python",
        backend="rust",
        level="reference_assisted_run",
        blocker="missing_table_observations",
        issue="#1738",
        findings=(
            ComparisonFinding(
                kind="report.tables.count",
                detail="different lengths",
                expected=4,
                actual=0,
            ),
        ),
    )
    return native, gap, path


def test_known_gap_is_a_visible_mismatch_and_cannot_satisfy_qualification(
    suite, known_gap
):
    examples, reference = suite
    native, gap, _ = known_gap
    assert qualify(examples, "revision-a", (reference, native)).errors
    inventory = qualify(examples, "revision-a", (reference, native), known_gaps=(gap,))
    assert not inventory.errors
    row = next(
        row for row in rows(inventory, "python", "rust") if row.example == gap.example
    )
    assert row.result == "semantic_mismatch"
    assert row.blockers == ("#1738:missing_table_observations",)
    assert row.findings == gap.findings
    required = ((gap.example, gap.runtime, gap.backend, gap.level),)
    assert qualify(
        examples,
        "revision-a",
        (reference, native),
        known_gaps=(gap,),
        required=required,
    ).errors


@pytest.mark.parametrize("mutation", ["changed", "unsupported", "fixed", "missing"])
def test_any_change_to_a_known_gap_requires_disposition(suite, known_gap, mutation):
    examples, reference = suite
    native, gap, path = known_gap
    payload = json.loads(path.read_text())
    if mutation == "missing":
        path.unlink()
    else:
        if mutation == "changed":
            payload["artifacts"][0]["content"] += "wrong\n"
        elif mutation == "unsupported":
            payload["outcome"] = "unsupported"
        else:
            original = json.loads(
                (
                    Path(reference.reports_dir) / "schema-lookup.python.python.json"
                ).read_text()
            )
            payload["tables"] = original["tables"]
        path.write_text(json.dumps(payload))
    inventory = qualify(examples, "revision-a", (reference, native), known_gaps=(gap,))
    assert any(
        "known gap changed or disappeared" in error for error in inventory.errors
    )


def test_reference_failures_cannot_be_declared_native_gaps(known_gap):
    _, gap, _ = known_gap
    payload = gap.model_dump(mode="json")
    payload.update(backend="python", level="reference_run")
    with pytest.raises(ValueError):
        KnownGap.model_validate_json(json.dumps(payload))


def test_duplicate_gaps_are_rejected(suite, known_gap):
    examples, reference = suite
    native, gap, _ = known_gap
    with pytest.raises(ValueError, match="duplicate known gap"):
        qualify(examples, "revision-a", (reference, native), known_gaps=(gap, gap))


def test_missing_entrypoint_is_explicit_without_fabricating_execution(suite):
    examples, reference = suite
    missing = MissingRoute(
        runtime="r", backend="rust", blocker="r_yaml_entrypoint_missing", issue="#1739"
    )
    inventory = qualify(examples, "revision-a", (reference,), missing_routes=(missing,))
    assert not inventory.errors
    for row in rows(inventory, "r", "rust"):
        assert row.result == "not_exercised"
        assert row.level is None and row.report is None
        assert row.blockers == ("#1739:r_yaml_entrypoint_missing",)
    with pytest.raises(ValueError, match="duplicate or contradictory"):
        qualify(examples, "revision-a", (reference,), missing_routes=(missing, missing))


def test_a_supplied_native_batch_cannot_also_claim_a_missing_route(suite, tmp_path):
    examples, reference = suite
    native = native_copy(suite, tmp_path)
    missing = MissingRoute(
        runtime="python", backend="rust", blocker="missing", issue="#1738"
    )
    with pytest.raises(ValueError, match="duplicate or contradictory"):
        qualify(examples, "revision-a", (reference, native), missing_routes=(missing,))
