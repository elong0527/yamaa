import pytest
from pathlib import Path
from cdiscbuilderv2.odm_parser import ODMParser

SAMPLE_ODM_XML = """<?xml version="1.0" encoding="UTF-8"?>
<ODM xmlns="http://www.cdisc.org/ns/odm/v1.3" FileOID="TEST.001">
  <Study OID="ST.001" StudyName="Test Study" ProtocolName="PROTO-01">
    <MetaDataVersion OID="MV.001" Name="Metadata v1.0">
      <ItemDef OID="IT.DM.AGE" Name="Age" DataType="integer">
        <Question><TranslatedText>Subject Age (yr)</TranslatedText></Question>
      </ItemDef>
      <ItemDef OID="IT.DM.SEX" Name="Sex" DataType="text">
        <Question><TranslatedText>Subject Sex</TranslatedText></Question>
      </ItemDef>
    </MetaDataVersion>
  </Study>
  <ClinicalData StudyOID="ST.001">
    <SubjectData SubjectKey="SUBJ-001">
      <StudyEventData StudyEventOID="SE.SCRN">
        <FormData FormOID="FO.DM">
          <ItemGroupData ItemGroupOID="IG.DM">
            <ItemData ItemOID="IT.DM.AGE" Value="45"/>
            <ItemData ItemOID="IT.DM.SEX" Value="Male"/>
          </ItemGroupData>
        </FormData>
      </StudyEventData>
    </SubjectData>
  </ClinicalData>
</ODM>
"""


def test_odm_parser_basic(tmp_path):
    xml_file = tmp_path / "test_odm.xml"
    xml_file.write_text(SAMPLE_ODM_XML)

    parser = ODMParser(xml_file)
    assert parser.study_info["ProtocolName"] == "PROTO-01"
    assert parser.df_long.height == 2
    assert "IT.DM.AGE" in parser.item_defs
    assert parser.item_defs["IT.DM.AGE"]["Question"] == "Subject Age (yr)"

    summary = parser.get_metadata_summary()
    assert summary.height == 2


def test_odm_parser_real_cath():
    cath_xml = Path("/home/ming/Documents/yamaa/cath/odm/odm.xml")
    if not cath_xml.exists():
        pytest.skip("CATH ODM XML file not found")

    parser = ODMParser(cath_xml)
    assert parser.study_info["OID"] == "ST.NCT00789880"
    assert parser.df_long.height > 5000
    assert len(parser.df_long["SubjectKey"].unique()) == 82
