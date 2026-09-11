from __future__ import annotations

import io
import tarfile
from pathlib import Path

import pytest
from conftest import ODM_13, ODM_20
from yamaa.odm.errors import ODMArchiveError, ODMMetadataError, ODMParseError
from yamaa.odm.profiles import KN189_ARCHIVE_MEMBER
from yamaa.odm.readers import iter_odm_records


def test_odm13_preserves_value_states_and_enriches_names(odm13_path: Path) -> None:
    rows = list(iter_odm_records(odm13_path))

    assert [row.source_ordinal for row in rows] == [1, 2, 3]
    assert [row.value for row in rows] == ["2026-01-02", "", None]
    assert [row.value_present for row in rows] == [True, True, False]
    assert [row.is_null for row in rows] == [False, False, True]
    assert rows[0].study_subject_id == "DISPLAY-1"
    assert rows[0].event_name == "Screening"
    assert rows[0].form_name == "Demographics"
    assert rows[0].item_group_name == "Subject details"
    assert rows[0].item_name == "Collection date"
    assert rows[0].item_group_repeat_key == "7"


def test_odm20_projects_outer_and_inner_groups(odm20_path: Path) -> None:
    rows = list(iter_odm_records(odm20_path))

    assert len(rows) == 2
    assert rows[0].study_subject_id is None
    assert rows[0].form_oid == "FO.20"
    assert rows[0].form_repeat_key == "2"
    assert rows[0].form_name == "Biomarker form"
    assert rows[0].item_group_oid == "IG.20"
    assert rows[0].item_group_repeat_key == "3"
    assert rows[0].item_group_name == "Biomarker group"
    assert rows[0].item_name == "Test name"
    assert rows[0].value == "PD-L1"
    assert rows[1].value == ""
    assert rows[1].value_present is True


def test_strict_metadata_scope_rejects_unresolved_item(tmp_path: Path) -> None:
    source = tmp_path / "unresolved.xml"
    source.write_text(
        ODM_20.replace(
            'ClinicalData StudyOID="S.20" MetaDataVersionOID="M.20"',
            'ClinicalData StudyOID="OTHER" MetaDataVersionOID="OTHER.M"',
        ),
        encoding="utf-8",
    )

    with pytest.raises(ODMMetadataError, match="unresolved ItemDef"):
        list(iter_odm_records(source))


def test_cart_profile_resolves_only_its_explicit_metadata_alias(tmp_path: Path) -> None:
    source = tmp_path / "cart-alias.xml"
    source.write_text(
        ODM_13.replace('Study OID="S.13"', 'Study OID="S_20204824(TEST)"')
        .replace('MetaDataVersion OID="M.13"', 'MetaDataVersion OID="v1.0.0"')
        .replace(
            'ClinicalData StudyOID="S.13" MetaDataVersionOID="M.13"',
            'ClinicalData StudyOID="S_DF(TEST)" MetaDataVersionOID="v1.0.0-S_DF(TEST)"',
        ),
        encoding="utf-8",
    )

    with pytest.raises(ODMMetadataError, match="unresolved ItemDef"):
        list(iter_odm_records(source, "strict"))

    rows = list(iter_odm_records(source, "cart-t-openclinica"))
    assert len(rows) == 3
    assert rows[0].study_oid == "S_DF(TEST)"
    assert rows[0].metadata_version_oid == "v1.0.0-S_DF(TEST)"
    assert rows[0].item_name == "Collection date"


def test_item_cannot_be_null_and_contain_a_value(tmp_path: Path) -> None:
    source = tmp_path / "conflicting-null.xml"
    source.write_text(
        ODM_13.replace(
            '<ItemData ItemOID="I.NULL" IsNull="Yes"/>',
            '<ItemData ItemOID="I.NULL" IsNull="Yes" Value="unexpected"/>',
        ),
        encoding="utf-8",
    )

    with pytest.raises(ODMParseError, match="null and also contains a value"):
        list(iter_odm_records(source))


def test_odm20_rejects_unsupported_group_depth(tmp_path: Path) -> None:
    source = tmp_path / "three-groups.xml"
    source.write_text(
        ODM_20.replace(
            '<ItemGroupData ItemGroupOID="IG.20" ItemGroupRepeatKey="3">',
            '<ItemGroupData ItemGroupOID="IG.20" ItemGroupRepeatKey="3">'
            '<ItemGroupData ItemGroupOID="EXTRA">',
        ).replace(
            "</ItemGroupData>\n        </ItemGroupData>",
            "</ItemGroupData></ItemGroupData>\n        </ItemGroupData>",
        ),
        encoding="utf-8",
    )

    with pytest.raises(ODMParseError, match="exactly two nested"):
        list(iter_odm_records(source))


def test_archive_profile_reads_only_named_regular_member(tmp_path: Path) -> None:
    archive_path = tmp_path / "odm.tar.gz"
    payload = ODM_20.encode()
    with tarfile.open(archive_path, "w:gz") as archive:
        info = tarfile.TarInfo(KN189_ARCHIVE_MEMBER)
        info.size = len(payload)
        archive.addfile(info, io.BytesIO(payload))
        html = b"<html/>"
        html_info = tarfile.TarInfo("KN189_odm_dag/index.html")
        html_info.size = len(html)
        archive.addfile(html_info, io.BytesIO(html))

    rows = list(iter_odm_records(archive_path, "kn189"))
    assert [row.item_oid for row in rows] == ["I.TEST", "I.RESULT"]


def test_archive_profile_rejects_symlink_member(tmp_path: Path) -> None:
    archive_path = tmp_path / "unsafe.tar.gz"
    with tarfile.open(archive_path, "w:gz") as archive:
        info = tarfile.TarInfo(KN189_ARCHIVE_MEMBER)
        info.type = tarfile.SYMTYPE
        info.linkname = "elsewhere.xml"
        archive.addfile(info)

    with pytest.raises(ODMArchiveError, match="not a regular file"):
        list(iter_odm_records(archive_path, "kn189"))


def test_malformed_xml_has_import_error(tmp_path: Path) -> None:
    source = tmp_path / "malformed.xml"
    source.write_text(
        '<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">', encoding="utf-8"
    )

    with pytest.raises(ODMParseError, match="malformed ODM XML"):
        list(iter_odm_records(source))
