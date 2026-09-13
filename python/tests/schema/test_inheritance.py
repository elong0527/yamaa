from __future__ import annotations

from pathlib import Path

import yaml

from yamaa import yamaa_domain
from yamaa.schema import resolve_specification
from yamaa.specification import SpecificationError, load_specification
from yamaa.specification.schema import load_schema_bundle

REPOSITORY = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY / "yaml"
EXAMPLES = SCHEMA_ROOT / "examples"


def test_committed_inheritance_example_matches_resolved_artifact() -> None:
    entry = EXAMPLES / "spec-inheritance/spec_study.yaml"
    resolved = resolve_specification(entry, load_schema_bundle(SCHEMA_ROOT))
    expected = yaml.safe_load(
        (EXAMPLES / "spec-inheritance/expected/spec_resolved.yaml").read_text(
            encoding="ascii"
        )
    )

    assert resolved.document == expected
    assert resolved.specification.parents is None
    assert [column.name for column in resolved.specification.columns] == [
        "USUBJID",
        "PARAMCD",
        "AVAL",
        "AVALU",
    ]
    assert (
        resolved.provenance["datasets.LB.path"].file
        == (EXAMPLES / "spec-inheritance/spec_organization.yaml").resolve()
    )


def test_public_loader_resolves_parented_entry() -> None:
    loaded = load_specification(
        EXAMPLES / "spec-inheritance/spec_study.yaml", SCHEMA_ROOT
    )

    assert loaded.specification.domain == "ADLB"
    assert loaded.specification.parents is None
    assert "UNUSED" not in loaded.specification.datasets


def test_inherited_source_literal_and_mapping_execute(tmp_path: Path) -> None:
    (tmp_path / "layers").mkdir()
    (tmp_path / "input.csv").write_text("ID,CODE\n01,A\n02,B\n", encoding="ascii")
    (tmp_path / "layers/parent.yaml").write_text(
        """schema_version: "1.0"
datasets:
  SRC: ../input.csv
base: SRC
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: SRC.ID}
  - name: CODE
    type: str
    label: Code
    derivation: {source: SRC.CODE}
  - name: CONSTANT
    type: str
    label: Constant
    derivation: {literal: fixed}
  - name: RESULT
    type: str
    label: Result
    derivation:
      mapping:
        source: CODE
        dict: {A: Alpha, B: Beta}
""",
        encoding="ascii",
    )
    (tmp_path / "spec.yaml").write_text(
        """schema_version: "1.0"
parents: layers/parent.yaml
domain: OUT
keys: [ID]
output:
  path: out.csv
  columns: [ID, CODE, CONSTANT, RESULT]
""",
        encoding="ascii",
    )

    run = yamaa_domain(tmp_path / "spec.yaml", schema_root=SCHEMA_ROOT)

    assert run.issues.is_empty()
    assert run.output is not None
    assert run.output.to_dicts() == [
        {"ID": "01", "CODE": "A", "CONSTANT": "fixed", "RESULT": "Alpha"},
        {"ID": "02", "CODE": "B", "CONSTANT": "fixed", "RESULT": "Beta"},
    ]


def test_parent_relative_path_is_rebased_without_changing_its_file(
    tmp_path: Path,
) -> None:
    first = tmp_path / "first"
    second = tmp_path / "second"
    child = tmp_path / "study"
    first.mkdir()
    second.mkdir()
    child.mkdir()
    (first / "data.csv").write_text("ID\nfirst\n", encoding="ascii")
    (second / "data.csv").write_text("ID\nsecond\n", encoding="ascii")
    parent_text = """schema_version: "1.0"
datasets: {SRC: data.csv}
base: SRC
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: SRC.ID}
"""
    (first / "parent.yaml").write_text(parent_text, encoding="ascii")
    (second / "parent.yaml").write_text(parent_text, encoding="ascii")
    entry = child / "spec.yaml"
    entry_text = """schema_version: "1.0"
parents: ../first/parent.yaml
domain: OUT
keys: [ID]
output: {path: out.csv, columns: [ID]}
"""
    entry.write_text(entry_text, encoding="ascii")
    bundle = load_schema_bundle(SCHEMA_ROOT)

    from_first = resolve_specification(entry, bundle)
    entry.write_text(entry_text.replace("first", "second"), encoding="ascii")
    from_second = resolve_specification(entry, bundle)

    assert from_first.specification.datasets["SRC"].path == "../first/data.csv"
    assert from_second.specification.datasets["SRC"].path == "../second/data.csv"
    assert (
        from_first.provenance["datasets.SRC.path"].file
        == (first / "parent.yaml").resolve()
    )
    assert (
        from_second.provenance["datasets.SRC.path"].file
        == (second / "parent.yaml").resolve()
    )


def test_inherited_source_does_not_widen_entry_project_root(tmp_path: Path) -> None:
    organization = tmp_path / "organization"
    study = tmp_path / "study"
    organization.mkdir()
    study.mkdir()
    (organization / "data.csv").write_text("ID\n01\n", encoding="ascii")
    (organization / "parent.yaml").write_text(
        """schema_version: "1.0"
datasets: {SRC: data.csv}
base: SRC
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: SRC.ID}
""",
        encoding="ascii",
    )
    (study / "spec.yaml").write_text(
        """schema_version: "1.0"
parents: ../organization/parent.yaml
domain: OUT
keys: [ID]
output: {path: out.csv, columns: [ID]}
""",
        encoding="ascii",
    )

    run = yamaa_domain(study / "spec.yaml", schema_root=SCHEMA_ROOT)

    assert run.output is None
    assert run.inputs == {}
    assert run.issues.row(0, named=True)["condition"] == (
        "resource_path_outside_project"
    )


def test_committed_inheritance_negatives_match_exact_diagnostics() -> None:
    bundle = load_schema_bundle(SCHEMA_ROOT)
    names = (
        "negative-adsl-cyclic-parent",
        "negative-adsl-invalid-parent-clear",
        "negative-adsl-parent-version-mismatch",
        "negative-adsl-remote-parent",
        "negative-adsl-inherited-output",
    )
    for name in names:
        try:
            resolve_specification(EXAMPLES / name / "spec.yaml", bundle)
        except SpecificationError as error:
            actual = error.diagnostics[0].model_dump(mode="json")
        else:
            raise AssertionError(f"{name} unexpectedly resolved")
        expected = yaml.safe_load(
            (EXAMPLES / name / "expected/error.yaml").read_text(encoding="ascii")
        )
        assert actual == expected
