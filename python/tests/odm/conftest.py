from __future__ import annotations

from pathlib import Path

import pytest

ODM_13 = """<?xml version="1.0" encoding="UTF-8"?>
<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3"
     xmlns:oc="https://www.openclinica.com/ns/odm_ext_v130/v3.1"
     ODMVersion="1.3" FileOID="FILE.13">
  <Study OID="S.13">
    <MetaDataVersion OID="M.13" Name="Metadata">
      <StudyEventDef OID="E.13" Name="Screening"/>
      <FormDef OID="F.13" Name="Demographics"/>
      <ItemGroupDef OID="G.13" Name="Subject details"/>
      <ItemDef OID="I.DATE" Name="Collection date" DataType="text"/>
      <ItemDef OID="I.EMPTY" Name="Collected empty" DataType="text"/>
      <ItemDef OID="I.NULL" Name="Explicit null" DataType="text"/>
    </MetaDataVersion>
  </Study>
  <ClinicalData StudyOID="S.13" MetaDataVersionOID="M.13">
    <SubjectData SubjectKey="SUBJECT-1" oc:StudySubjectID="DISPLAY-1"
                 oc:Status="Available">
      <StudyEventData StudyEventOID="E.13" oc:StartDate="2026-01-02">
        <FormData FormOID="F.13">
          <ItemGroupData ItemGroupOID="G.13" ItemGroupRepeatKey="7"
                         TransactionType="Insert">
            <ItemData ItemOID="I.DATE" Value="2026-01-02"/>
            <ItemData ItemOID="I.EMPTY" Value=""/>
            <ItemData ItemOID="I.NULL" IsNull="Yes"/>
          </ItemGroupData>
        </FormData>
      </StudyEventData>
    </SubjectData>
  </ClinicalData>
</ODM>
"""


ODM_20 = """<?xml version="1.0" encoding="UTF-8"?>
<ODM xmlns="http://www.cdisc.org/ns/odm/v2.0"
     ODMVersion="2.0" FileOID="FILE.20">
  <Study OID="S.20">
    <MetaDataVersion OID="M.20" Name="Metadata">
      <StudyEventDef OID="E.20" Name="Screening"/>
      <ItemGroupDef OID="FO.20" Name="Biomarker form"/>
      <ItemGroupDef OID="IG.20" Name="Biomarker group"/>
      <ItemDef OID="I.TEST" Name="Test name" DataType="text"/>
      <ItemDef OID="I.RESULT" Name="Test result" DataType="text"/>
    </MetaDataVersion>
  </Study>
  <ClinicalData StudyOID="S.20" MetaDataVersionOID="M.20">
    <SubjectData SubjectKey="SUBJECT-20">
      <StudyEventData StudyEventOID="E.20">
        <ItemGroupData ItemGroupOID="FO.20" ItemGroupRepeatKey="2">
          <ItemGroupData ItemGroupOID="IG.20" ItemGroupRepeatKey="3">
            <ItemData ItemOID="I.TEST"><Value>PD-L1</Value></ItemData>
            <ItemData ItemOID="I.RESULT"><Value/></ItemData>
          </ItemGroupData>
        </ItemGroupData>
      </StudyEventData>
    </SubjectData>
  </ClinicalData>
</ODM>
"""


@pytest.fixture
def odm13_path(tmp_path: Path) -> Path:
    path = tmp_path / "odm13.xml"
    path.write_text(ODM_13, encoding="utf-8")
    return path


@pytest.fixture
def odm20_path(tmp_path: Path) -> Path:
    path = tmp_path / "odm20.xml"
    path.write_text(ODM_20, encoding="utf-8")
    return path
