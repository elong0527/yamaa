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


def test_rejects_bare_submission_comment(tmp_path: Path) -> None:
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    old = (
        "  comment:\n"
        "    text: Demographics are described in section 2.1 of the reviewer's guide.\n"
        "    documents: [{document: csdrg}]\n"
    )
    assert old in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(
            old,
            "  comment: Demographics are described in section 2.1.\n",
            1,
        ),
        encoding="ascii",
    )

    with pytest.raises(SpecificationError) as caught:
        load_specification(path, SCHEMA_ROOT)

    diagnostic = caught.value.diagnostics[0].model_dump(mode="json")
    assert diagnostic["condition"] == "invalid_field_type"
    assert diagnostic["spec_paths"] == ["submission.comment"]


def test_rejects_bare_submission_method(tmp_path: Path) -> None:
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    old = (
        "      method:\n"
        "        description: Study identifier, site identifier, and subject identifier joined by hyphens.\n"
    )
    assert old in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(
            old,
            "      method: Study identifier joined by hyphens.\n",
            1,
        ),
        encoding="ascii",
    )

    with pytest.raises(SpecificationError) as caught:
        load_specification(path, SCHEMA_ROOT)

    diagnostic = caught.value.diagnostics[0].model_dump(mode="json")
    assert diagnostic["condition"] == "invalid_field_type"
    assert diagnostic["spec_paths"] == ["columns.USUBJID.submission.method"]


def test_rejects_bare_document_reference(tmp_path: Path) -> None:
    source = (EXAMPLE / "spec.yaml").read_text(encoding="ascii")
    old = "    documents: [{document: csdrg}]\n"
    assert old in source
    path = tmp_path / "spec.yaml"
    path.write_text(
        source.replace(
            old,
            "    documents: [{document: csdrg}, bare-ref]\n",
            1,
        ),
        encoding="ascii",
    )

    with pytest.raises(SpecificationError) as caught:
        load_specification(path, SCHEMA_ROOT)

    diagnostic = caught.value.diagnostics[0].model_dump(mode="json")
    assert diagnostic["condition"] == "invalid_field_type"
    assert diagnostic["spec_paths"] == ["submission.comment.documents[1]"]
