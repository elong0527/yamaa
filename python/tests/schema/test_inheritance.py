from __future__ import annotations

from pathlib import Path

import polars as pl
import yaml
from polars.testing import assert_frame_equal

from yamaa import yamaa_domain
from yamaa.schema import resolve_specification
from yamaa.specification import SpecificationError, load_specification
from yamaa.specification.schema import load_schema_bundle

REPOSITORY = Path(__file__).parents[3]
SCHEMA_ROOT = REPOSITORY / "yaml"
EXAMPLES = REPOSITORY / "benchmarks"


def test_committed_inheritance_example_matches_resolved_artifact() -> None:
    entry = EXAMPLES / "schema-inheritance/spec_study.yaml"
    resolved = resolve_specification(entry, load_schema_bundle(SCHEMA_ROOT))
    expected = yaml.safe_load(
        (EXAMPLES / "schema-inheritance/expected/spec_resolved.yaml").read_text(
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
        resolved.provenance["input.LB.path"].file
        == (EXAMPLES / "schema-inheritance/spec_organization.yaml").resolve()
    )


def test_committed_column_composition_example_matches_resolved_artifact() -> None:
    example = EXAMPLES / "schema-column-composition"
    resolved = resolve_specification(
        example / "spec_study.yaml", load_schema_bundle(SCHEMA_ROOT)
    )
    expected = yaml.safe_load(
        (example / "expected/spec_resolved.yaml").read_text(encoding="ascii")
    )

    assert resolved.document == expected
    # REQ-0616 keeps provenance at the leaf, because the composed ANRIND
    # dictionary and AVAL annotations come from three different layers.
    assert (
        resolved.provenance["columns.ANRIND.derivation.value.mapping.dict.L"].file
        == (example / "spec_compound.yaml").resolve()
    )
    assert (
        resolved.provenance["columns.ANRIND.derivation.value.mapping.dict.N"].file
        == (example / "spec_organization.yaml").resolve()
    )
    assert (
        resolved.provenance["columns.AVAL.metadata.review_status"].file
        == (example / "spec_study.yaml").resolve()
    )


def test_committed_inherited_output_example_matches_resolved_artifact() -> None:
    example = EXAMPLES / "schema-inherited-output"
    resolved = resolve_specification(
        example / "spec_study.yaml", load_schema_bundle(SCHEMA_ROOT)
    )
    expected = yaml.safe_load(
        (example / "expected/spec_resolved.yaml").read_text(encoding="ascii")
    )

    assert resolved.document == expected
    assert "output" not in yaml.safe_load(
        (example / "spec_study.yaml").read_text(encoding="ascii")
    )


def test_committed_column_composition_example_executes_to_its_artifact() -> None:
    example = EXAMPLES / "schema-column-composition"

    run = yamaa_domain(example / "spec_study.yaml", schema_root=SCHEMA_ROOT)

    assert run.issues.is_empty()
    assert run.output is not None
    committed = pl.read_csv(example / "expected/adlb.csv", schema=run.output.schema)
    assert_frame_equal(run.output, committed, check_exact=True)


def test_column_member_composes_by_declared_kind(tmp_path: Path) -> None:
    (tmp_path / "input.csv").write_text("ID,CODE\n01,a\n", encoding="ascii")
    (tmp_path / "parent.yaml").write_text(
        """schema_version: "1.0"
input: {SRC: input.csv}
base: SRC
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: SRC.ID}
  - name: RESULT
    type: str
    label: Result
    derivation:
      mapping:
        source: SRC.CODE
        dict: {A: Alpha}
        case_sensitive: false
    verifications:
      - max_length: {max: 8}
    metadata: {analysis_role: result, origin_note: parent}
""",
        encoding="ascii",
    )
    (tmp_path / "spec.yaml").write_text(
        """schema_version: "1.0"
parents: parent.yaml
domain: OUT
keys: [ID]
output: {path: out.csv, columns: [ID, RESULT]}
columns:
  - name: RESULT
    derivation:
      mapping:
        dict: {B: Beta}
        missing: null
    verifications:
      - not_missing: {}
    metadata: {origin_note: study, reviewed: "yes"}
""",
        encoding="ascii",
    )

    resolved = resolve_specification(
        tmp_path / "spec.yaml", load_schema_bundle(SCHEMA_ROOT)
    )
    result = resolved.document["columns"][1]

    # A mapping composes key by key, a class field by field, and a schema
    # default is materialized on the composed value, so the parent's
    # case_sensitive survives a child that never mentions it.
    assert result["derivation"] == {
        "value": {
            "mapping": {
                "source": "SRC.CODE",
                "dict": {"A": "Alpha", "B": "Beta"},
                "case_sensitive": False,
                "missing": None,
            }
        }
    }
    assert result["metadata"] == {
        "analysis_role": "result",
        "origin_note": "study",
        "reviewed": "yes",
    }
    # Every list replaces, so the child's one check is the whole list.
    assert result["verifications"] == [{"not_missing": {"severity": "error"}}]


def test_child_expression_naming_another_keyword_replaces_the_derivation(
    tmp_path: Path,
) -> None:
    (tmp_path / "input.csv").write_text("ID,CODE\n01,A\n", encoding="ascii")
    (tmp_path / "parent.yaml").write_text(
        """schema_version: "1.0"
input: {SRC: input.csv}
base: SRC
columns:
  - name: ID
    type: str
    label: Identifier
    derivation: {source: SRC.ID}
  - name: RESULT
    type: str
    label: Result
    derivation:
      mapping:
        source: SRC.CODE
        dict: {A: Alpha}
""",
        encoding="ascii",
    )
    (tmp_path / "spec.yaml").write_text(
        """schema_version: "1.0"
parents: parent.yaml
domain: OUT
keys: [ID]
output: {path: out.csv, columns: [ID, RESULT]}
columns:
  - name: RESULT
    derivation: {literal: fixed}
""",
        encoding="ascii",
    )

    resolved = resolve_specification(
        tmp_path / "spec.yaml", load_schema_bundle(SCHEMA_ROOT)
    )

    assert resolved.document["columns"][1]["derivation"] == {
        "value": {"literal": "fixed"}
    }


def test_row_member_field_still_replaces_whole(tmp_path: Path) -> None:
    (tmp_path / "input.csv").write_text("ID,CODE\n01,a\n", encoding="ascii")
    (tmp_path / "parent.yaml").write_text(
        """schema_version: "1.0"
input: {SRC: input.csv}
rows:
  - id: only
    dataset: SRC
    derivations:
      ID: {source: SRC.ID}
      CODE:
        mapping:
          source: SRC.CODE
          dict: {A: Alpha}
          case_sensitive: false
columns:
  - name: ID
    type: str
    label: Identifier
  - name: CODE
    type: str
    label: Code
""",
        encoding="ascii",
    )
    (tmp_path / "spec.yaml").write_text(
        """schema_version: "1.0"
parents: parent.yaml
domain: OUT
keys: [ID]
output: {path: out.csv, columns: [ID, CODE]}
rows:
  - id: only
    derivations:
      ID: {source: SRC.ID}
      CODE:
        mapping:
          source: SRC.CODE
          dict: {B: Beta}
""",
        encoding="ascii",
    )

    resolved = resolve_specification(
        tmp_path / "spec.yaml", load_schema_bundle(SCHEMA_ROOT)
    )

    # REQ-0630 composes only a columns member. A rows member field is still
    # replaced whole, so the inherited entry and the inherited
    # case_sensitive are both gone rather than composed.
    assert resolved.document["rows"][0]["derivations"]["CODE"] == {
        "value": {
            "mapping": {
                "source": "SRC.CODE",
                "dict": {"B": "Beta"},
                "case_sensitive": True,
            }
        }
    }


def test_public_loader_resolves_parented_entry() -> None:
    loaded = load_specification(
        EXAMPLES / "schema-inheritance/spec_study.yaml", SCHEMA_ROOT
    )

    assert loaded.specification.domain == "ADLB"
    assert loaded.specification.parents is None
    assert "UNUSED" not in loaded.specification.input


def test_inherited_source_literal_and_mapping_execute(tmp_path: Path) -> None:
    (tmp_path / "layers").mkdir()
    (tmp_path / "input.csv").write_text("ID,CODE\n01,A\n02,B\n", encoding="ascii")
    (tmp_path / "layers/parent.yaml").write_text(
        """schema_version: "1.0"
input:
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
input: {SRC: data.csv}
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

    assert from_first.specification.input["SRC"].path == "../first/data.csv"
    assert from_second.specification.input["SRC"].path == "../second/data.csv"
    assert (
        from_first.provenance["input.SRC.path"].file
        == (first / "parent.yaml").resolve()
    )
    assert (
        from_second.provenance["input.SRC.path"].file
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
input: {SRC: data.csv}
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


def test_entry_inherits_output_from_a_shared_layer(tmp_path: Path) -> None:
    common = tmp_path / "common"
    study = tmp_path / "study"
    common.mkdir()
    (study / "input").mkdir(parents=True)
    (tmp_path / "yamaa-project.yaml").write_text('version: "1.0"\n', encoding="ascii")
    (study / "input/src.csv").write_text("ID\nS1\n", encoding="ascii")
    (common / "base.yaml").write_text(
        """schema_version: "1.0"
domain: OUT
keys: [ID]
output:
  path: out.csv
  columns: [ID]
  warning_log: warnings.csv
columns:
  - name: ID
    type: str
    derivation: SRC.ID
""",
        encoding="ascii",
    )
    (study / "spec.yaml").write_text(
        """schema_version: "1.0"
parents: [../common/base.yaml]
input:
  SRC: input/src.csv
base: SRC
""",
        encoding="ascii",
    )

    resolved = resolve_specification(
        study / "spec.yaml", load_schema_bundle(SCHEMA_ROOT)
    )
    run = yamaa_domain(study / "spec.yaml", schema_root=SCHEMA_ROOT)

    # Issue #845: the entry declares no output and inherits it whole
    # (REQ-0623), publishing where the shared layer names (REQ-0629).
    assert resolved.document["output"] == {
        "path": "../common/out.csv",
        "columns": ["ID"],
        "warning_log": "../common/warnings.csv",
    }
    assert run.issues.is_empty()
    assert run.output is not None
    assert run.output.to_dicts() == [{"ID": "S1"}]


def test_entry_output_replaces_inherited_output_whole(tmp_path: Path) -> None:
    (tmp_path / "input.csv").write_text("ID,CODE\n01,A\n", encoding="ascii")
    (tmp_path / "parent.yaml").write_text(
        """schema_version: "1.0"
domain: OUT
keys: [ID]
input: {SRC: input.csv}
base: SRC
output:
  path: shared.csv
  columns: [ID, CODE]
  warning_log: shared-warnings.csv
columns:
  - name: ID
    type: str
    derivation: SRC.ID
  - name: CODE
    type: str
    derivation: SRC.CODE
""",
        encoding="ascii",
    )
    (tmp_path / "spec.yaml").write_text(
        """schema_version: "1.0"
parents: parent.yaml
output: {path: out.csv, columns: [ID]}
""",
        encoding="ascii",
    )

    resolved = resolve_specification(
        tmp_path / "spec.yaml", load_schema_bundle(SCHEMA_ROOT)
    )

    # REQ-0629: no inherited output field survives an entry's own output.
    assert resolved.document["output"] == {"path": "out.csv", "columns": ["ID"]}
    assert [column.name for column in resolved.specification.columns] == ["ID"]


def test_output_no_layer_declares_fails_as_missing_required_field(
    tmp_path: Path,
) -> None:
    (tmp_path / "parent.yaml").write_text(
        """schema_version: "1.0"
domain: OUT
keys: [ID]
columns:
  - name: ID
    type: str
    derivation: SRC.ID
""",
        encoding="ascii",
    )
    (tmp_path / "spec.yaml").write_text(
        """schema_version: "1.0"
parents: parent.yaml
input: {SRC: input.csv}
base: SRC
""",
        encoding="ascii",
    )

    try:
        resolve_specification(tmp_path / "spec.yaml", load_schema_bundle(SCHEMA_ROOT))
    except SpecificationError as error:
        diagnostic = error.diagnostics[0]
    else:
        raise AssertionError("a chain with no output unexpectedly resolved")

    # REQ-0657: the requiredness is structural and applies after resolution.
    assert diagnostic.condition == "missing_required_field"
    assert diagnostic.spec_paths == ("output",)


def test_committed_inheritance_negatives_match_exact_diagnostics() -> None:
    bundle = load_schema_bundle(SCHEMA_ROOT)
    names = (
        "negative-cyclic-parent",
        "negative-property-clear",
        "negative-version-mismatch",
        "negative-remote-parent",
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


def test_a_column_reading_an_intermediate_depends_on_its_key_base() -> None:
    from yamaa.schema.inheritance import _column_dependencies

    bundle = load_schema_bundle(SCHEMA_ROOT)
    intermediates = {
        "LOOK": {
            "id": "LOOK",
            "dataset": "RIGHT",
            "key_base": ["MATCHKEY"],
            "key": ["TESTCD"],
        }
    }
    column = {"name": "V", "type": "float", "derivation": {"source": "LOOK.V"}}

    # REQ-0050: the intermediate's match values are dependencies of every
    # column that reads it, so R017 orders MATCHKEY before V.
    assert _column_dependencies(column, [], intermediates, bundle) == {"MATCHKEY"}
