"""R028 IG domain-model conformance checks."""

from __future__ import annotations

from pathlib import Path

import pytest

from yamaa.ig import (
    IgConformanceError,
    check_dataset_conformance,
    check_study_document,
    family_for_standard,
    load_template,
)
from yamaa.specification.models import Specification

REPO_ROOT = Path(__file__).parents[3]
TEMPLATES_ROOT = REPO_ROOT / "yaml" / "templates" / "ig"
SCHEMA_ROOT = REPO_ROOT / "yaml"

# Every SDTMIG 3.4 DM Req variable, so the conforming fixture stays honest
# against the template it is checked with.
DM_REQUIRED = ["STUDYID", "DOMAIN", "USUBJID", "SUBJID", "SITEID", "SEX"]


def _specification(
    domain: str = "DM",
    output_columns: list[str] | None = None,
    cores: dict[str, str] | None = None,
) -> Specification:
    columns = output_columns if output_columns is not None else DM_REQUIRED
    cores = cores or {}
    return Specification.model_validate(
        {
            "schema_version": "1.0",
            "domain": domain,
            "input": {"RAW": {"path": "input/raw.csv"}},
            "keys": ["STUDYID", "USUBJID"],
            "output": {"path": "dm.csv", "columns": list(columns)},
            "columns": [
                {
                    "name": name,
                    "type": "str",
                    "derivation": {"value": {"literal": "x"}},
                    **({"submission": {"core": cores[name]}} if name in cores else {}),
                }
                for name in columns
            ],
        }
    )


def _check(specification: Specification) -> list:
    return check_dataset_conformance(
        specification,
        standard_name="SDTMIG",
        standard_version="3.4",
        templates_root=TEMPLATES_ROOT,
    )


def test_family_table() -> None:
    assert family_for_standard("SDTMIG") == "sdtm"
    assert family_for_standard("SENDIG-DART") == "send"
    assert family_for_standard("ADaMIG") == "adam"
    assert family_for_standard("BIMO") is None


def test_template_loads() -> None:
    template = load_template(TEMPLATES_ROOT, "sdtm", "SDTMIG", "3.4", "DM")
    assert template is not None
    assert template.key == "sdtm/SDTMIG/3.4/DM"
    assert load_template(TEMPLATES_ROOT, "sdtm", "SDTMIG", "3.4", "XX") is None


def test_conforming_dm_reports_expected_signals_only() -> None:
    diagnostics = _check(_specification())
    assert not [item for item in diagnostics if item.severity == "error"]
    # Every Exp variable absent from this minimal spec is a signal, never a failure.
    assert {item.condition for item in diagnostics} == {"ig_missing_expected"}


def test_missing_required_fails() -> None:
    columns = [name for name in DM_REQUIRED if name != "SEX"]
    diagnostics = _check(_specification(output_columns=columns))
    failures = [item for item in diagnostics if item.condition == "ig_missing_required"]
    assert len(failures) == 1
    assert failures[0].severity == "error"
    assert failures[0].requirement == "R028-6"
    assert failures[0].context["variable"] == "SEX"


def test_missing_expected_is_signal() -> None:
    diagnostics = _check(_specification())
    signals = [item for item in diagnostics if item.condition == "ig_missing_expected"]
    assert signals
    assert {item.severity for item in signals} == {"signal"}
    assert all(item.requirement == "R028-7" for item in signals)


def test_unknown_variable_with_misspelling_hint() -> None:
    diagnostics = _check(_specification(output_columns=[*DM_REQUIRED, "RACEE"]))
    matches = [item for item in diagnostics if item.condition == "ig_unknown_variable"]
    assert len(matches) == 1
    assert matches[0].severity == "signal"
    assert matches[0].context["suggestion"] == "RACE"


def test_unknown_variable_without_hint() -> None:
    diagnostics = _check(_specification(output_columns=[*DM_REQUIRED, "ZZTOP"]))
    matches = [item for item in diagnostics if item.condition == "ig_unknown_variable"]
    assert len(matches) == 1
    assert "suggestion" not in matches[0].context


def test_core_mismatch_is_signal() -> None:
    diagnostics = _check(_specification(cores={"SEX": "Exp", "STUDYID": "Req"}))
    matches = [item for item in diagnostics if item.condition == "ig_core_mismatch"]
    assert len(matches) == 1
    assert matches[0].severity == "signal"
    assert matches[0].context == {
        "variable": "SEX",
        "declared": "Exp",
        "template": "Req",
    }


def test_no_template_is_explicit_signal() -> None:
    diagnostics = _check(_specification(domain="XX"))
    assert len(diagnostics) == 1
    assert diagnostics[0].condition == "ig_no_template"
    assert diagnostics[0].severity == "signal"


def test_adam_family_is_out_of_scope() -> None:
    specification = _specification()
    diagnostics = check_dataset_conformance(
        specification,
        standard_name="ADaMIG",
        standard_version="2.1",
        templates_root=TEMPLATES_ROOT,
    )
    assert diagnostics == []


def test_study_document_composition() -> None:
    define_path = REPO_ROOT / "benchmark" / "sdtm-dm-metadata-contract" / "define.yaml"
    diagnostics = check_study_document(define_path, schema_root=SCHEMA_ROOT)
    # The DM spec declares every SDTMIG 3.4 Req variable, so composition finds
    # signals (missing Exp variables, core disagreements) but no failures.
    assert not [item for item in diagnostics if item.severity == "error"]
    assert {item.condition for item in diagnostics} <= {
        "ig_missing_expected",
        "ig_core_mismatch",
        "ig_no_template",
    }


def test_study_document_composition_raises_on_missing_required(
    tmp_path: Path,
) -> None:
    spec_path = tmp_path / "spec.yaml"
    spec_path.write_text(
        """\
schema_version: '1.0'
domain: DM
keys: [STUDYID, USUBJID]
input:
  RAW: input/raw.csv
output:
  path: dm.csv
  columns: [DOMAIN, USUBJID]
columns:
  - name: DOMAIN
    type: str
    derivation:
      literal: DM
  - name: USUBJID
    type: str
    derivation:
      literal: x
""",
        encoding="ascii",
    )
    define_path = tmp_path / "define.yaml"
    define_path.write_text(
        """\
schema_version: '1.0'
standards:
  - id: SDTMIG
    name: SDTMIG
    type: IG
    version: '3.4'
default_standard: SDTMIG
datasets:
  - id: DM
    spec: spec.yaml
""",
        encoding="ascii",
    )
    with pytest.raises(IgConformanceError) as error:
        check_study_document(define_path, schema_root=SCHEMA_ROOT)
    assert any(
        item.condition == "ig_missing_required" for item in error.value.diagnostics
    )
