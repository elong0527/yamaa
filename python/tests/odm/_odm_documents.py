"""Shared ODM documents used by the odm tests and fixtures.

This module exists so test modules can import ``ODM_13``/``ODM_20`` without
going through ``conftest``: two ``conftest.py`` files (``functions/`` and
``odm/``) share the top-level ``conftest`` module name under pytest's
prepend import mode, so ``from conftest import ...`` resolves to whichever
conftest loaded last and breaks collection depending on invocation order.
A uniquely named private module sidesteps that collision entirely.
"""

from __future__ import annotations

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
