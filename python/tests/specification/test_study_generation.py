from __future__ import annotations

import json
import shutil
from copy import deepcopy
from pathlib import Path

import polars as pl
import pytest
import yaml

from yamaa import generate_study_document, yamaa_domain
from yamaa.io import SourceError
from yamaa.specification import SpecificationError
from yamaa.specification.models import (
    Row,
    SubmissionColumn,
    SubmissionOrigin,
)
from yamaa.specification.terminology import LoadedStudyDocument
from yamaa.submission import (
    compose_study,
    load_study_document,
    render_define_xml,
    validate_define_xml,
)

ROOT = Path(__file__).parents[3]
SCHEMA = ROOT / "yaml"
EXAMPLE = ROOT / "benchmarks/sdtm-dm-metadata"


@pytest.fixture
def study():
    return load_study_document(EXAMPLE / "define.yaml", SCHEMA)


def test_define_xml_matches_independent_golden(study):
    composed = compose_study(study)
    content = render_define_xml(composed)
    assert content == (EXAMPLE / "expected/define.xml").read_bytes()
    assert content == render_define_xml(compose_study(study))
    assert study.specifications["DM"].columns[2].submission.length is None
    validate_define_xml(content)


def test_generate_package_from_published_dataset(tmp_path):
    shutil.copytree(EXAMPLE, tmp_path, dirs_exist_ok=True)
    pilot = yamaa_domain(
        tmp_path / "spec.yaml",
        schema_root=SCHEMA,
        study_document=tmp_path / "define.yaml",
    )
    assert pilot.issues.is_empty()
    pilot.save()
    output = generate_study_document(tmp_path / "define.yaml", schema_root=SCHEMA)
    assert output.read_bytes() == (EXAMPLE / "expected/define.xml").read_bytes()
    expected = EXAMPLE / "expected/dm.json"
    assert (tmp_path / "dm.json").read_bytes() == expected.read_bytes()


def test_generation_resolves_inherited_specifications(tmp_path):
    shutil.copytree(EXAMPLE, tmp_path, dirs_exist_ok=True)
    (tmp_path / "spec.yaml").rename(tmp_path / "parent.yaml")
    (tmp_path / "spec.yaml").write_text(
        'schema_version: "1.0"\nparents: [parent.yaml]\n'
    )
    pilot = yamaa_domain(
        tmp_path / "spec.yaml",
        schema_root=SCHEMA,
        study_document=tmp_path / "define.yaml",
    )
    assert pilot.issues.is_empty()
    pilot.save()
    output = generate_study_document(tmp_path / "define.yaml", schema_root=SCHEMA)
    assert output.read_bytes() == (EXAMPLE / "expected/define.xml").read_bytes()
    assert (tmp_path / "dm.json").read_bytes() == (
        EXAMPLE / "expected/dm.json"
    ).read_bytes()


def test_artifact_outside_entry_directory_uses_explicit_project_root(tmp_path):
    shutil.copytree(EXAMPLE, tmp_path, dirs_exist_ok=True)
    pilot = yamaa_domain(tmp_path / "spec.yaml", schema_root=SCHEMA)
    pilot.save()
    document = yaml.safe_load((tmp_path / "define.yaml").read_text())
    document["output"]["path"] = "../define.xml"
    document["datasets"][0]["spec"] = "../spec.yaml"
    metadata = tmp_path / "metadata"
    metadata.mkdir()
    (metadata / "define.yaml").write_text(yaml.safe_dump(document, sort_keys=False))
    output = generate_study_document(
        metadata / "define.yaml", schema_root=SCHEMA, project_root=tmp_path
    )
    assert output.read_bytes() == (EXAMPLE / "expected/define.xml").read_bytes()
    assert (tmp_path / "dm.json").read_bytes() == (
        EXAMPLE / "expected/dm.json"
    ).read_bytes()


@pytest.mark.parametrize(
    "mutation,condition",
    [
        (
            lambda doc: doc["datasets"].append(deepcopy(doc["datasets"][0])),
            "duplicate_define_identifier",
        ),
        (
            lambda doc: doc["datasets"].append({"id": "DM2", "spec": "spec.yaml"}),
            "duplicate_dataset_entry",
        ),
        (lambda doc: doc.update(default_standard="UNKNOWN"), "unknown_standard"),
        (lambda doc: doc.update(default_standard="CT_SDTM"), "unknown_standard_family"),
        (
            lambda doc: doc["documents"][0].update(id="DM"),
            "duplicate_define_identifier",
        ),
        (lambda doc: doc.update(documents=[]), "origin_document_missing"),
        (
            lambda doc: doc["standards"][0].update(publishing_set="SDTM"),
            "value_not_permitted",
        ),
        (
            lambda doc: doc["datasets"][0].update(dataset_json="define.xml"),
            "dataset_json_path_collision",
        ),
        (
            lambda doc: doc["datasets"][0].update(dataset_json="nested/dm.json"),
            "artifact_outside_document",
        ),
    ],
)
def test_composition_rejects_invalid_documents(study, mutation, condition):
    document = deepcopy(study.document)
    mutation(document)
    specifications = dict(study.specifications)
    specifications["DM2"] = specifications["DM"]
    with pytest.raises(SpecificationError) as caught:
        compose_study(LoadedStudyDocument(document, study.written_path, specifications))
    assert condition in {
        diagnostic.condition for diagnostic in caught.value.diagnostics
    }


@pytest.mark.parametrize(
    "family,origin_type,source,core,mandatory,valid",
    [
        ("SDTMIG", "Assigned", "Sponsor", "Req", None, True),
        ("SDTMIG", "Assigned", "Investigator", "Req", None, False),
        ("SDTMIG", "Assigned", "Sponsor", "Cond", None, False),
        ("SENDIG", "Assigned", None, "Req", None, True),
        ("SENDIG", "Assigned", "Sponsor", "Req", None, False),
        ("ADaMIG", "Assigned", None, "Req", False, True),
        ("ADaMIG", "Assigned", "Sponsor", "Req", False, False),
        ("ADaMIG", "Assigned", None, "Exp", False, False),
        ("ADaMIG", "Assigned", None, "Req", None, False),
    ],
)
def test_family_rules_and_resolved_defaults(
    study, family, origin_type, source, core, mandatory, valid
):
    document = deepcopy(study.document)
    document["standards"][0]["name"] = family
    document["codelists"] = []
    spec = study.specifications["DM"]
    original = spec.columns[0]
    column = original.model_copy(
        update={
            "submission": original.submission.model_copy(
                update={
                    "core": core,
                    "mandatory": mandatory,
                    "role": None,
                    "codelist": None,
                    "origin": SubmissionOrigin(type=origin_type, source=source),
                }
            )
        }
    )
    spec = spec.model_copy(
        update={
            "columns": [column],
            "output": spec.output.model_copy(update={"columns": [column.name]}),
            "keys": [column.name],
        }
    )
    selected = LoadedStudyDocument(document, study.written_path, {"DM": spec})
    if not valid:
        with pytest.raises(SpecificationError):
            compose_study(selected)
        return
    composed = compose_study(selected)
    resolved = composed.datasets[0].columns[0].submission
    assert resolved.mandatory is (family != "ADaMIG")
    assert resolved.origin.source == (None if family == "SENDIG" else "Sponsor")
    validate_define_xml(render_define_xml(composed))


def test_escaping_and_declared_stylesheet(study):
    document = deepcopy(study.document)
    document.update(stylesheet="style.xsl", originator='A&B <C> "D"')
    document["study"]["description"] = "A&B <C>\nnext"
    content = render_define_xml(
        compose_study(
            LoadedStudyDocument(document, study.written_path, study.specifications)
        )
    )
    assert b'<?xml-stylesheet type="text/xsl" href="style.xsl"?>\n' in content
    assert b'Originator="A&amp;B &lt;C&gt; &quot;D&quot;"' in content
    assert b"A&amp;B &lt;C&gt;\nnext" in content


@pytest.mark.parametrize(
    "text,requirement",
    [("bad\rtext", "REQ-1025"), ("bad\ttext", "REQ-1025"), ("bad\x00text", "REQ-1026")],
)
def test_invalid_text_is_rejected_before_publication(study, text, requirement):
    document = deepcopy(study.document)
    document["study"]["description"] = text
    with pytest.raises(SpecificationError) as caught:
        render_define_xml(
            compose_study(
                LoadedStudyDocument(document, study.written_path, study.specifications)
            )
        )
    assert caught.value.diagnostics[0].requirement == requirement


@pytest.mark.parametrize("target", ["column_label", "document_title"])
def test_invalid_text_reports_the_declaration_carrying_it(study, target):
    document = deepcopy(study.document)
    specifications = dict(study.specifications)
    if target == "column_label":
        spec = specifications["DM"]
        columns = list(spec.columns)
        columns[0] = columns[0].model_copy(update={"label": "bad\tlabel"})
        specifications["DM"] = spec.model_copy(update={"columns": columns})
        expected = f"datasets.DM.columns.{columns[0].name}.label"
    else:
        entry = document["documents"][0]
        entry["title"] = "bad\ttitle"
        expected = f"documents.{entry['id']}.title"
    with pytest.raises(SpecificationError) as caught:
        render_define_xml(
            compose_study(
                LoadedStudyDocument(document, study.written_path, specifications)
            )
        )
    assert caught.value.diagnostics[0].spec_paths == (expected,)


def test_runtime_binding_enforced_without_explicit_allowed_values(tmp_path):
    shutil.copytree(EXAMPLE, tmp_path, dirs_exist_ok=True)
    specification = yaml.safe_load((tmp_path / "spec.yaml").read_text())
    sex = next(column for column in specification["columns"] if column["name"] == "SEX")
    sex["verifications"] = [{"not_missing": {}}]
    (tmp_path / "spec.yaml").write_text(yaml.safe_dump(specification, sort_keys=False))
    frame = pl.read_csv(tmp_path / "input/dm_raw.csv").with_columns(
        pl.lit("X").alias("SEX")
    )
    frame.write_csv(tmp_path / "input/dm_raw.csv")
    pilot = yamaa_domain(
        tmp_path / "spec.yaml",
        schema_root=SCHEMA,
        study_document=tmp_path / "define.yaml",
    )
    assert pilot.output is None
    issue = pilot.issues.row(0, named=True)
    assert issue["phase"] == "verification"
    assert issue["condition"] == "allowed_values_failed"
    context = json.loads(issue["context"])
    assert context["column"] == "SEX" and context["values"] == ["X"] * 3
    assert context["keys"][0]["USUBJID"]


def test_invalid_generation_preserves_existing_document(tmp_path):
    shutil.copytree(EXAMPLE, tmp_path, dirs_exist_ok=True)
    (tmp_path / "define.xml").write_bytes(b"previous document")
    with pytest.raises(SourceError):
        generate_study_document(tmp_path / "define.yaml", schema_root=SCHEMA)
    assert (tmp_path / "define.xml").read_bytes() == b"previous document"
    assert not (tmp_path / "dm.json").exists()
    assert not list(tmp_path.glob("*.part"))


def test_document_target_symlink_is_rejected_without_touching_its_referent(tmp_path):
    shutil.copytree(EXAMPLE, tmp_path, dirs_exist_ok=True)
    pilot = yamaa_domain(tmp_path / "spec.yaml", schema_root=SCHEMA)
    pilot.save()
    referent = tmp_path / "existing.xml"
    referent.write_bytes(b"keep these bytes")
    (tmp_path / "define.xml").symlink_to(referent)
    with pytest.raises(ValueError, match="regular file"):
        generate_study_document(tmp_path / "define.yaml", schema_root=SCHEMA)
    assert referent.read_bytes() == b"keep these bytes"
    assert not (tmp_path / "dm.json").exists()


def test_value_metadata_is_rejected_with_row_and_column(study):
    spec = study.specifications["DM"]
    spec = spec.model_copy(
        update={
            "rows": [
                Row(
                    id="value",
                    derivations={},
                    submission={
                        "SEX": SubmissionColumn(
                            origin=SubmissionOrigin(type="Assigned", source="Sponsor")
                        )
                    },
                )
            ]
        }
    )
    with pytest.raises(SpecificationError) as caught:
        compose_study(
            LoadedStudyDocument(study.document, study.written_path, {"DM": spec})
        )
    diagnostic = next(
        item for item in caught.value.diagnostics if item.requirement == "REQ-1169"
    )
    assert diagnostic.spec_paths == ("datasets.DM.rows.value.submission.SEX",)
    assert (
        diagnostic.context["row"] == "value" and diagnostic.context["column"] == "SEX"
    )


def test_collision_between_dataset_and_column_comment_identifiers(study):
    document = deepcopy(study.document)
    document["context"] = "Other"
    document["datasets"] = [
        {"id": "DM", "spec": "spec.yaml"},
        {"id": "DM.AGEU", "spec": "second.yaml"},
    ]
    second = study.specifications["DM"].model_copy(
        update={
            "output": study.specifications["DM"].output.model_copy(
                update={"path": "second.csv"}
            )
        }
    )
    with pytest.raises(SpecificationError) as caught:
        compose_study(
            LoadedStudyDocument(
                document,
                study.written_path,
                {"DM": study.specifications["DM"], "DM.AGEU": second},
            )
        )
    diagnostic = next(
        item for item in caught.value.diagnostics if item.requirement == "REQ-0971"
    )
    assert diagnostic.context["identifier"] == "COM.DM.AGEU"
