from __future__ import annotations

import io
import tarfile
from pathlib import Path

import pytest
from conftest import ODM_13, ODM_20
from yamaa.odm.errors import ODMError
from yamaa.odm.readers import iter_odm_records, read_odm
from yamaa.odm.schema import ODM_ITEM_SCHEMA


def test_odm13_preserves_values_and_enriches_exact_metadata(odm13_path: Path) -> None:
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


def test_two_groups_supply_form_and_item_group_context(odm20_path: Path) -> None:
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


def test_read_odm_returns_fixed_schema_polars_frame(odm20_path: Path) -> None:
    frame = read_odm(odm20_path)

    assert frame.schema == ODM_ITEM_SCHEMA
    assert frame.height == 2
    assert frame["SourceOrdinal"].to_list() == [1, 2]


def test_missing_exact_metadata_leaves_names_null(tmp_path: Path) -> None:
    source = tmp_path / "unresolved.xml"
    source.write_text(
        ODM_20.replace(
            'ClinicalData StudyOID="S.20" MetaDataVersionOID="M.20"',
            'ClinicalData StudyOID="OTHER" MetaDataVersionOID="OTHER.M"',
        ),
        encoding="utf-8",
    )

    rows = list(iter_odm_records(source))

    assert rows[0].study_oid == "OTHER"
    assert rows[0].metadata_version_oid == "OTHER.M"
    assert rows[0].event_name is None
    assert rows[0].form_name is None
    assert rows[0].item_group_name is None
    assert rows[0].item_name is None


def test_item_cannot_be_null_and_contain_a_value(tmp_path: Path) -> None:
    source = tmp_path / "conflicting-null.xml"
    source.write_text(
        ODM_13.replace(
            '<ItemData ItemOID="I.NULL" IsNull="Yes"/>',
            '<ItemData ItemOID="I.NULL" IsNull="Yes" Value="unexpected"/>',
        ),
        encoding="utf-8",
    )

    with pytest.raises(ODMError, match="null and contains a value"):
        list(iter_odm_records(source))


def test_unsupported_group_depth_is_rejected(tmp_path: Path) -> None:
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

    with pytest.raises(ODMError, match="requires FormData plus one"):
        list(iter_odm_records(source))


def test_archive_with_one_xml_member_is_selected_automatically(
    tmp_path: Path,
) -> None:
    archive_path = tmp_path / "odm.tar.gz"
    payload = ODM_20.encode()
    with tarfile.open(archive_path, "w:gz") as archive:
        info = tarfile.TarInfo("study/odm.xml")
        info.size = len(payload)
        archive.addfile(info, io.BytesIO(payload))
        html = b"<html/>"
        html_info = tarfile.TarInfo("study/index.html")
        html_info.size = len(html)
        archive.addfile(html_info, io.BytesIO(html))
        sidecar = b"apple-double"
        sidecar_info = tarfile.TarInfo("study/._odm.xml")
        sidecar_info.size = len(sidecar)
        archive.addfile(sidecar_info, io.BytesIO(sidecar))

    rows = list(iter_odm_records(archive_path))
    assert [row.item_oid for row in rows] == ["I.TEST", "I.RESULT"]


def test_archive_member_selects_one_of_multiple_xml_files(tmp_path: Path) -> None:
    archive_path = tmp_path / "multiple.tar.gz"
    payload = ODM_20.encode()
    with tarfile.open(archive_path, "w:gz") as archive:
        for name in ("first.xml", "nested/second.xml"):
            info = tarfile.TarInfo(name)
            info.size = len(payload)
            archive.addfile(info, io.BytesIO(payload))

    with pytest.raises(ODMError, match="exactly one XML member"):
        list(iter_odm_records(archive_path))

    rows = list(iter_odm_records(archive_path, archive_member="nested/second.xml"))
    assert len(rows) == 2


def test_archive_rejects_non_regular_xml_member(tmp_path: Path) -> None:
    archive_path = tmp_path / "unsafe.tar.gz"
    with tarfile.open(archive_path, "w:gz") as archive:
        info = tarfile.TarInfo("study/odm.xml")
        info.type = tarfile.SYMTYPE
        info.linkname = "elsewhere.xml"
        archive.addfile(info)

    with pytest.raises(ODMError, match="not a regular file"):
        list(iter_odm_records(archive_path))


def test_malformed_xml_has_helper_error(tmp_path: Path) -> None:
    source = tmp_path / "malformed.xml"
    source.write_text(
        '<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3">', encoding="utf-8"
    )

    with pytest.raises(ODMError, match="malformed ODM XML"):
        list(iter_odm_records(source))
