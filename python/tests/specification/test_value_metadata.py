from __future__ import annotations

from pathlib import Path

import polars as pl
import pytest
from polars.testing import assert_frame_equal

from yamaa import yamaa_domain
from yamaa.specification import SpecificationError, load_specification

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"
EXAMPLE = REPOSITORY_ROOT / "benchmarks" / "sdtm-lb-value-level-metadata"


def _mutated(tmp_path: Path, old: str, new: str) -> Path:
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    assert old in source
    path = tmp_path / "spec.yaml"
    path.write_text(source.replace(old, new, 1), encoding="ascii")
    return path


def _condition(path: Path) -> tuple[str, tuple[str, ...], str | None]:
    with pytest.raises(SpecificationError) as caught:
        load_specification(path, SCHEMA_ROOT)
    diagnostic = caught.value.diagnostics[0].model_dump(mode="json")
    return (
        diagnostic["condition"],
        tuple(diagnostic["spec_paths"]),
        diagnostic["requirement"],
    )


def test_loads_value_level_metadata() -> None:
    loaded = load_specification(EXAMPLE / "spec.yaml", SCHEMA_ROOT)
    specification = loaded.specification

    columns = {column.name: column for column in specification.columns}
    assert columns["LBORRESU"].submission is not None
    assert columns["LBORRESU"].submission.codelist == "CL_LBORRESU"

    rows = {row.id: row for row in specification.rows}
    glucose = rows["glucose"].submission["LBORRESU"]
    assert glucose.codelist == "CL_GLUCOSE"
    assert glucose.origin.type == "Assigned"
    assert glucose.origin.source == "Vendor"
    creatinine = rows["creatinine"].submission["LBORRESU"]
    assert creatinine.codelist == "CL_CREATININE"
    assert creatinine.origin.source == "Investigator"


def test_executes_value_level_metadata() -> None:
    pilot = yamaa_domain(EXAMPLE / "spec.yaml", schema_root=SCHEMA_ROOT)

    assert pilot.spec is not None
    assert pilot.output is not None
    assert pilot.issues.is_empty()
    committed = pl.read_csv(EXAMPLE / "expected" / "lb.csv", schema=pilot.output.schema)
    assert_frame_equal(pilot.output, committed, check_exact=True)


def test_rejects_dynamic_discriminator(tmp_path: Path) -> None:
    path = _mutated(
        tmp_path,
        "      LBTESTCD:\n        literal: GLUC",
        "      LBTESTCD: ODM.TestCode",
    )

    assert _condition(path) == (
        "value_metadata_requires_literal_testcd",
        ("rows.glucose.submission",),
        "REQ-1164",
    )


def test_rejects_conflicting_value_metadata(tmp_path: Path) -> None:
    path = _mutated(
        tmp_path,
        "      LBTESTCD:\n        literal: CREAT",
        "      LBTESTCD:\n        literal: GLUC",
    )

    assert _condition(path) == (
        "conflicting_value_metadata",
        ("rows.creatinine.submission.LBORRESU",),
        "REQ-1167",
    )


def test_accepts_identical_duplicate_declarations(tmp_path: Path) -> None:
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    old_testcd = "      LBTESTCD:\n        literal: CREAT"
    assert old_testcd in source
    old_submission = (
        "        origin:\n"
        "          type: Assigned\n"
        "          source: Investigator\n"
        "        codelist: CL_CREATININE"
    )
    assert old_submission in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(old_testcd, "      LBTESTCD:\n        literal: GLUC", 1).replace(
            old_submission,
            "        origin:\n"
            "          type: Assigned\n"
            "          source: Vendor\n"
            "        codelist: CL_GLUCOSE",
            1,
        ),
        encoding="ascii",
    )

    loaded = load_specification(path, SCHEMA_ROOT)
    assert loaded.specification.rows is not None


def test_rejects_unknown_submission_column(tmp_path: Path) -> None:
    path = _mutated(
        tmp_path,
        "    submission:\n      LBORRESU:",
        "    submission:\n      BOGUS:\n"
        "        origin:\n"
        "          type: Assigned\n"
        "      LBORRESU:",
    )

    assert _condition(path) == (
        "value_metadata_unknown_column",
        ("rows.glucose.submission.BOGUS",),
        "REQ-1165",
    )


def test_rejects_refuted_value_origin(tmp_path: Path) -> None:
    path = _mutated(
        tmp_path,
        "          type: Assigned\n          source: Vendor",
        "          type: Collected\n          source: Vendor",
    )

    assert _condition(path) == (
        "refuted_value_origin",
        ("rows.glucose.submission.LBORRESU",),
        "REQ-1168",
    )


def test_rejects_non_output_column(tmp_path: Path) -> None:
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    old_columns = "LBSTRESN, LBSTRESU, LBDTC]"
    assert old_columns in source
    old_submission = "    submission:\n      LBORRESU:"
    assert old_submission in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(old_columns, "LBSTRESN, LBDTC]", 1).replace(
            old_submission,
            "    submission:\n"
            "      LBSTRESU:\n"
            "        origin:\n"
            "          type: Assigned\n"
            "      LBORRESU:",
            1,
        ),
        encoding="ascii",
    )

    assert _condition(path) == (
        "value_metadata_not_output_column",
        ("rows.glucose.submission.LBSTRESU",),
        "REQ-1166",
    )


def test_rejects_submission_without_testcd_column(tmp_path: Path) -> None:
    path = _mutated(tmp_path, "domain: LB", "domain: XX")

    assert _condition(path) == (
        "value_metadata_requires_testcd",
        ("rows.glucose.submission",),
        "REQ-1163",
    )


def test_rejects_unknown_row_submission_subfield(tmp_path: Path) -> None:
    path = _mutated(
        tmp_path,
        "        codelist: CL_GLUCOSE",
        "        codelist: CL_GLUCOSE\n        bogus_field: oops",
    )

    condition, spec_paths, _ = _condition(path)
    assert condition in ("unknown_field", "model_contract_mismatch")
    assert spec_paths == ("rows.glucose.submission.LBORRESU.bogus_field",)
