from __future__ import annotations

from pathlib import Path

import polars as pl
import pytest
from polars.testing import assert_frame_equal

from yamaa import yamaa_domain
from yamaa.specification import SpecificationError, load_specification

REPOSITORY_ROOT = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY_ROOT / "yaml"
EXAMPLE = REPOSITORY_ROOT / "benchmarks" / "sdtm-dm-metadata"


def test_loads_metadata_contract_submission() -> None:
    loaded = load_specification(EXAMPLE / "spec.yaml", SCHEMA_ROOT)
    specification = loaded.specification

    assert specification.submission is not None
    assert specification.submission.label == "Demographics"
    assert specification.submission.class_name == "SPECIAL PURPOSE"
    assert specification.submission.repeating is False
    assert specification.submission.reference_data is False

    columns = {column.name: column for column in specification.columns}
    assert columns["DOMAIN"].submission is not None
    assert columns["DOMAIN"].submission.origin.type == "Assigned"
    assert columns["USUBJID"].submission is not None
    assert columns["USUBJID"].submission.origin.type == "Derived"
    assert columns["SUBJID"].submission is not None
    assert columns["SUBJID"].submission.origin.type == "Collected"


def test_executes_metadata_contract_submission() -> None:
    pilot = yamaa_domain(EXAMPLE / "spec.yaml", schema_root=SCHEMA_ROOT)

    assert pilot.spec is not None
    assert pilot.output is not None
    assert pilot.issues.is_empty()
    committed = pl.read_csv(EXAMPLE / "expected" / "dm.csv", schema=pilot.output.schema)
    assert_frame_equal(pilot.output, committed, check_exact=True)


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


def test_rejects_unknown_column_submission_subfield(tmp_path: Path) -> None:
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    old = "      core: Req\n      role: Identifier\n      length: 2\n"
    assert old in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(old, old + "      bogus_field: oops\n", 1),
        encoding="ascii",
    )

    with pytest.raises(SpecificationError) as caught:
        load_specification(path, SCHEMA_ROOT)

    diagnostic = caught.value.diagnostics[0].model_dump(mode="json")
    assert diagnostic["condition"] in ("unknown_field", "model_contract_mismatch")
    assert diagnostic["spec_paths"] == ["columns.DOMAIN.submission.bogus_field"]


def test_rejects_unadmitted_data_type(tmp_path: Path) -> None:
    # REQ-0868: `str` never admits `integer`.
    path = _mutated(
        tmp_path,
        "  - name: DOMAIN\n    type: str",
        "  - name: DOMAIN\n    type: str",
    )
    source = path.read_text(encoding="ascii")
    path.write_text(
        source.replace(
            "      length: 2\n", "      data_type: integer\n      length: 2\n", 1
        ),
        encoding="ascii",
    )

    assert _condition(path) == (
        "submission_data_type_not_admitted",
        ("columns.DOMAIN.submission",),
        "REQ-0914",
    )


def test_rejects_missing_length(tmp_path: Path) -> None:
    # REQ-0872: `text` requires a length when no max_length derives it.
    path = _mutated(
        tmp_path,
        "      role: Identifier\n      length: 2\n",
        "      role: Identifier\n",
    )

    assert _condition(path) == (
        "submission_length_missing",
        ("columns.DOMAIN.submission",),
        "REQ-0912",
    )


def test_rejects_length_where_prohibited(tmp_path: Path) -> None:
    # REQ-0872: a `date` submission type carries fixed-form values.
    path = _mutated(
        tmp_path,
        "      core: Req\n      role: Identifier\n      length: 2\n",
        "      core: Req\n      role: Identifier\n      data_type: date\n      length: 2\n",
    )

    assert _condition(path) == (
        "submission_length_not_applicable",
        ("columns.DOMAIN.submission",),
        "REQ-0912",
    )


def test_rejects_declared_length_conflict(tmp_path: Path) -> None:
    # REQ-0875: declaring both is accepted when equal, rejected when different.
    # USUBJID derives length 30 from max_length; declaring 31 conflicts.
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    old = (
        "      core: Req\n      role: Identifier\n      origin:\n        type: Derived"
    )
    assert old in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(
            old,
            "      core: Req\n      role: Identifier\n      length: 31\n      origin:\n        type: Derived",
            1,
        ),
        encoding="ascii",
    )

    assert _condition(path) == (
        "declared_length_conflict",
        ("columns.USUBJID.submission",),
        "REQ-0913",
    )


def test_rejects_non_positive_length(tmp_path: Path) -> None:
    # REQ-0871: length is a positive integer.
    path = _mutated(tmp_path, "      length: 2\n", "      length: 0\n")

    assert _condition(path) == (
        "submission_length_invalid",
        ("columns.DOMAIN.submission.length",),
        "REQ-0911",
    )


def test_rejects_missing_core_when_root_submission_present(tmp_path: Path) -> None:
    # REQ-0882/REQ-0915, scoped per #1514 to specs opting in via root submission.
    path = _mutated(
        tmp_path,
        "      core: Req\n      role: Identifier\n",
        "      role: Identifier\n",
    )

    assert _condition(path) == (
        "core_mandatory_conflict",
        ("columns.DOMAIN.submission",),
        "REQ-0915",
    )


def test_rejects_mandatory_without_enforcement(tmp_path: Path) -> None:
    # REQ-0885: mandatory:true needs not_missing or keys. AGEU is Exp with
    # no verifications, so declaring it mandatory fails.
    path = _mutated(
        tmp_path,
        "  - name: AGEU\n    type: str",
        "  - name: AGEU\n    type: str",
    )
    source = path.read_text(encoding="ascii")
    old = "      core: Exp\n      role: Variable Qualifier\n      length: 5\n"
    assert old in source
    path.write_text(
        source.replace(old, old + "      mandatory: true\n", 1),
        encoding="ascii",
    )

    assert _condition(path) == (
        "mandatory_not_enforced",
        ("columns.AGEU.submission",),
        "REQ-0916",
    )


def test_rejects_missing_origin(tmp_path: Path) -> None:
    # REQ-0887: every column with submission metadata declares an origin.
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    old = "      length: 2\n      codelist: DOMAIN\n      origin:\n        type: Assigned\n        source: Sponsor\n"
    assert old in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(old, "      length: 2\n      codelist: DOMAIN\n", 1),
        encoding="ascii",
    )

    assert _condition(path) == (
        "origin_missing",
        ("columns.DOMAIN.submission",),
        "REQ-0918",
    )


def test_rejects_missing_origin_description(tmp_path: Path) -> None:
    # REQ-0892: Predecessor/Other/Not Available require a description.
    path = _mutated(
        tmp_path,
        "        type: Assigned\n        source: Sponsor",
        "        type: Other\n        source: Sponsor",
    )

    assert _condition(path) == (
        "origin_description_missing",
        ("columns.DOMAIN.submission",),
        "REQ-0920",
    )


def test_rejects_missing_method_for_derived(tmp_path: Path) -> None:
    # REQ-0895: a Derived origin claims an algorithm.
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    assert "      method:" in source
    path = tmp_path / "spec.yaml"
    lines = source.splitlines(keepends=True)
    kept = [line for line in lines if not line.lstrip().startswith("method:")]
    # The folded continuation line belongs to the method; drop it too.
    kept = [line for line in kept if "by hyphens." not in line]
    path.write_text("".join(kept), encoding="ascii")

    condition, spec_paths, requirement = _condition(path)
    assert (condition, spec_paths, requirement) == (
        "method_missing",
        ("columns.USUBJID.submission",),
        "REQ-0923",
    )


def test_rejects_contradicted_origin(tmp_path: Path) -> None:
    # REQ-0898: a bare literal admits Assigned/Protocol/Other, not Collected.
    path = _mutated(
        tmp_path,
        "        type: Assigned\n        source: Sponsor",
        "        type: Collected\n        source: Sponsor",
    )

    assert _condition(path) == (
        "origin_contradicts_derivation",
        ("columns.DOMAIN.submission",),
        "REQ-0922",
    )


def test_rejects_submission_outside_output(tmp_path: Path) -> None:
    # REQ-0856: submission only for output.columns.
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    old = (
        "  columns: [DOMAIN, STUDYID, USUBJID, SUBJID, SITEID, AGE, AGEU, SEX, COUNTRY]"
    )
    assert old in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(
            old,
            "  columns: [DOMAIN, STUDYID, USUBJID, SUBJID, SITEID, AGE, AGEU, SEX]",
            1,
        ),
        encoding="ascii",
    )

    assert _condition(path) == (
        "submission_not_output_column",
        ("columns.COUNTRY.submission",),
        "REQ-0909",
    )


def test_rejects_reserved_metadata_key(tmp_path: Path) -> None:
    # REQ-0858: the free-form map must not carry a governed key.
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    assert "\nsubmission:\n  label:" in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(
            "\nsubmission:\n  label:",
            "\nmetadata:\n  label: smuggled\nsubmission:\n  label:",
            1,
        ),
        encoding="ascii",
    )

    assert _condition(path) == (
        "reserved_metadata_key",
        ("metadata.label",),
        "REQ-0910",
    )


def test_rejects_reference_data_with_repeating(
    tmp_path: Path,
) -> None:  # REQ-0865/REQ-0925: reference data never repeats.
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    old = "  repeating: false\n"
    assert old in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(old, "  repeating: true\n  reference_data: true\n", 1),
        encoding="ascii",
    )

    assert _condition(path) == (
        "reference_data_repeating_conflict",
        ("submission",),
        "REQ-0925",
    )


def test_intermediate_lookup_admits_derived_origin() -> None:
    # A reference through a named intermediate joins and selects a record,
    # so it computes rather than copies: Derived is admitted (REQ-0897),
    # while a bare input-dataset source refutes it (REQ-0899).
    example = REPOSITORY_ROOT / "benchmarks" / "sdtm-ae-coding"
    loaded = load_specification(example / "spec.yaml", SCHEMA_ROOT)
    columns = {column.name: column for column in loaded.specification.columns}
    assert columns["AEDECOD"].submission is not None
    assert columns["AEDECOD"].submission.origin.type == "Derived"
    assert columns["AEDECOD"].submission.method is not None
