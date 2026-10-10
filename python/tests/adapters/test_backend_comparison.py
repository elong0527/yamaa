"""Mutations prove backend comparisons see behavior hidden by final artifacts."""

import copy
import json
import shutil
from pathlib import Path

import pytest
import yaml
from pydantic import ValidationError

from yamaa.adapters.conformance import (
    ExampleReport,
    compare_example,
    compare_reports,
    execute_example,
    main,
    read_report,
    write_report,
)
from yamaa.adapters.observations import CallbackObservation, observe_scalar

ROOT = Path(__file__).parents[3]
EXAMPLES = ROOT / "benchmarks"


@pytest.fixture(scope="module")
def reports(tmp_path_factory):
    destination = tmp_path_factory.mktemp("backend-reports")
    return {
        name: execute_example(
            EXAMPLES / name,
            schema_root=ROOT / "yaml",
            output_dir=destination / name,
        )
        for name in (
            "schema-verification-log",
            "schema-parquet",
            "sdtm-dm-race-ethnicity",
            "negative-integer-overflow",
        )
    }


def changed(report, mutate):
    payload = copy.deepcopy(report.model_dump(mode="json"))
    mutate(payload)
    return ExampleReport.model_validate_json(json.dumps(payload))


def test_host_and_backend_identity_do_not_change_portable_observations(reports):
    reference = reports["schema-verification-log"]
    # Protocol compatibility only: this does not claim an R/Rust execution.
    candidate = reference.model_copy(
        update={
            "runtime": "r",
            "runtime_version": "4.6.1",
            "backend": "rust",
            "engine_version": "prototype",
        }
    )
    assert compare_reports(reference, candidate).passed
    assert compare_example(candidate, EXAMPLES / candidate.example).backend == "rust"


@pytest.fixture
def callback_report(reports):
    # Independently authored protocol input, not evidence of reference execution.
    # Actual versionless callbacks and activation vectors are qualified by the
    # installed shared public environment suite.
    return reports["schema-verification-log"].model_copy(
        update={
            "callbacks": tuple(
                CallbackObservation(
                    specification="spec.yaml",
                    function="protocol_fixture",
                    contract_version="component-fixture",
                    arguments=(("value", observe_scalar(value)),),
                )
                for value in (1.0, 2.0)
            )
        }
    )


def test_callback_protocol_keeps_order_and_exact_arguments(callback_report):
    assert [call.arguments for call in callback_report.callbacks] == [
        (("value", observe_scalar(1.0)),),
        (("value", observe_scalar(2.0)),),
    ]


@pytest.mark.parametrize(
    "mutation",
    [
        "callback_count",
        "callback_order",
        "value",
        "type",
        "row_order",
        "missing",
        "source_reads",
    ],
)
def test_observation_drift_fails_even_when_artifacts_are_identical(
    callback_report, mutation
):
    reference = callback_report

    def mutate(payload):
        if mutation == "callback_count":
            payload["callbacks"].pop()
        elif mutation == "source_reads":
            payload["source_reads"].pop()
        elif mutation == "callback_order":
            payload["callbacks"].reverse()
        else:
            table = next(
                table for table in payload["tables"] if table["stage"] == "derived"
            )
            if mutation == "value":
                table["rows"][0][-1] = observe_scalar(25.000000001).model_dump()
            elif mutation == "type":
                table["types"][-1] = "str"
            elif mutation == "row_order":
                table["rows"].reverse()
            else:
                table["rows"][0][-1] = observe_scalar(None).model_dump()

    candidate = changed(reference, mutate)
    assert candidate.artifacts == reference.artifacts
    assert compare_example(candidate, EXAMPLES / candidate.example).passed
    assert not compare_reports(reference, candidate).passed


def test_verification_ledger_records_held_and_violated_checks(reports):
    report = reports["schema-verification-log"]
    assert len(report.verifications) == 4
    assert [record.failure is None for record in report.verifications] == [
        True,
        False,
        True,
        True,
    ]
    candidate = changed(
        report, lambda data: data["verifications"][0].update(evaluated_count=999)
    )
    assert not compare_reports(report, candidate).passed


def test_checks_are_observed_without_requesting_a_sidecar(tmp_path):
    case = tmp_path / "case"
    shutil.copytree(EXAMPLES / "schema-verification-log", case)
    path = case / "spec.yaml"
    document = yaml.safe_load(path.read_text())
    del document["output"]["warning_log"]
    del document["output"]["verification_log"]
    # Warning declarations require a warning log. Use a held error check for
    # this independent no-sidecar case; all three fixture ages are nonnegative.
    document["columns"][1]["verifications"][1]["range"] = {"min": 0, "max": 250}
    path.write_text(yaml.safe_dump(document, sort_keys=False))
    report = execute_example(
        case, schema_root=ROOT / "yaml", output_dir=tmp_path / "out"
    )
    assert report.outcome == "success", report
    assert len(report.verifications) == 4
    assert [artifact.name for artifact in report.artifacts] == ["adsl"]


def test_producer_tables_and_handler_counts_are_observed(reports):
    report = reports["sdtm-dm-race-ethnicity"]
    assert len(report.nodes) == 2
    assert {table.specification for table in report.tables} == {
        "spec_dm.yaml",
        "spec_suppdm.yaml",
    }
    candidate = changed(report, lambda data: data["nodes"].pop(0))
    assert not compare_reports(report, candidate).passed


def test_parquet_compares_logical_data_without_a_byte_length_guarantee(reports):
    report = reports["schema-parquet"]
    candidate = changed(
        report, lambda data: data["artifacts"][0].update(byte_length=999999)
    )
    assert compare_reports(report, candidate).passed
    candidate = changed(
        candidate, lambda data: data["artifacts"][0]["records"].reverse()
    )
    assert not compare_reports(report, candidate).passed


def test_portable_diagnostics_are_compared(reports):
    report = reports["negative-integer-overflow"]
    candidate = changed(
        report, lambda data: data["diagnostics"][0].update(condition="other")
    )
    assert not compare_reports(report, candidate).passed


@pytest.mark.parametrize("outcome", ["unsupported", "error"])
def test_equal_non_executions_cannot_establish_parity(reports, outcome):
    report = reports["schema-verification-log"].model_copy(update={"outcome": outcome})
    assert not compare_reports(report, report).passed


@pytest.mark.parametrize("value", [-(2**63), 2**63 - 1, 2**53 + 1])
def test_i64_transport_is_exact_decimal_text(value):
    observed = observe_scalar(value)
    assert observed.type == "int"
    assert observed.value == str(value)


def test_float_transport_preserves_bits_and_missing_differs_from_empty_text():
    assert observe_scalar(0.1).value == "3fb999999999999a"
    assert observe_scalar(0.0) != observe_scalar(-0.0)
    assert observe_scalar(None) != observe_scalar("")


@pytest.mark.parametrize("mutation", ["version", "missing_observations"])
def test_old_or_incomplete_reports_fail_explicitly(reports, tmp_path, mutation):
    payload = reports["schema-verification-log"].model_dump(mode="json")
    if mutation == "version":
        payload["report_version"] = "0.1.0-draft"
    else:
        del payload["callbacks"]
    path = tmp_path / "report.json"
    path.write_text(json.dumps(payload))
    with pytest.raises(ValidationError):
        read_report(path)


def test_report_names_keep_different_backends_separate(reports, tmp_path):
    report = reports["schema-verification-log"]
    other = report.model_copy(update={"runtime": "r", "backend": "rust"})
    assert write_report(report, tmp_path) != write_report(other, tmp_path)
    assert read_report(write_report(other, tmp_path)) == other


def test_cli_fails_when_reference_observations_differ(reports, tmp_path):
    report = reports["schema-verification-log"]
    candidate = changed(report, lambda data: data["verifications"].pop())
    reference_dir = tmp_path / "references"
    write_report(candidate, reference_dir)
    assert (
        main(
            [
                report.example,
                "--examples-root",
                str(EXAMPLES),
                "--run-dir",
                str(tmp_path / "run"),
                "--reference-reports",
                str(reference_dir),
            ]
        )
        == 1
    )


def test_failed_producer_keeps_its_own_log_and_evaluated_checks(tmp_path):
    import shutil

    import yaml

    example = tmp_path / "example"
    shutil.copytree(EXAMPLES / "schema-verification-log", example)
    producer = example / "producer.yaml"
    (example / "spec.yaml").rename(producer)
    document = yaml.safe_load(producer.read_text())
    document["columns"][1]["verifications"][1]["range"]["severity"] = "error"
    producer.write_text(yaml.safe_dump(document, sort_keys=False))
    (example / "spec.yaml").write_text("""schema_version: "1.0"
domain: FINAL
input:
  A: {path: adsl.csv, schema: producer.yaml}
base: A
keys: [USUBJID]
output: {path: final.csv, columns: [USUBJID]}
columns:
  - {name: USUBJID, type: str, label: Identifier, derivation: A.USUBJID}
""")
    report = execute_example(
        example, schema_root=ROOT / "yaml", output_dir=tmp_path / "output"
    )
    assert report.outcome == "failure"
    assert [node.specification for node in report.nodes] == ["producer.yaml"]
    assert [artifact.name for artifact in report.artifacts] == ["adsl-checks"]
    assert len(report.verifications) == 2
    assert report.verifications[-1].failure is not None
    assert {table.stage for table in report.tables} == {"source"}
    assert not (tmp_path / "output/final.csv").exists()
    assert not (tmp_path / "output/adsl.csv").exists()
