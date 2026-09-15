import pytest
from pathlib import Path
from cdiscbuilderv2.odm_parser import ODMParser
from cdiscbuilderv2.ai_generator import AISDTMSchemaGenerator
from cdiscbuilderv2.engine import CDISCEngine
import yaml


def test_ai_generator_discovery():
    cath_xml = Path("/home/ming/Documents/yamaa/cath/odm/odm.xml")
    if not cath_xml.exists():
        pytest.skip("CATH XML not found")

    parser = ODMParser(cath_xml)
    generator = AISDTMSchemaGenerator(
        metadata_df=parser.get_metadata_summary(),
        df_long=parser.df_long
    )

    discovered = generator.discover_domains()
    assert len(discovered) >= 4

    domain_names = [d["domain"] for d in discovered]
    assert "DM" in domain_names
    assert "VS" in domain_names
    assert "AE" in domain_names
    assert "LB" in domain_names


def test_ai_generator_synthesis_and_execution():
    cath_xml = Path("/home/ming/Documents/yamaa/cath/odm/odm.xml")
    if not cath_xml.exists():
        pytest.skip("CATH XML not found")

    parser = ODMParser(cath_xml)
    generator = AISDTMSchemaGenerator(
        metadata_df=parser.get_metadata_summary(),
        df_long=parser.df_long
    )

    schemas = generator.generate_all_schemas()
    assert "DM" in schemas
    assert "VS" in schemas

    # Test DM schema execution
    dm_yaml = schemas["DM"]
    dm_spec = yaml.safe_load(dm_yaml)
    assert dm_spec["domain"] == "DM"
    assert dm_spec["base"] == "ODM"

    engine = CDISCEngine(dm_spec, df_long=parser.df_long)
    dm_df = engine.build()
    assert dm_df.height == 82
    assert "USUBJID" in dm_df.columns
    assert "SEX" in dm_df.columns
    assert "AGE" in dm_df.columns
    assert "ARM" in dm_df.columns

    # Verify first row values are accurate and not null / 'U'
    row1 = dm_df.row(0, named=True)
    assert row1["STUDYID"] == "ST.NCT00789880"
    assert row1["USUBJID"] == "CATH-UCSD-0001"
    assert row1["SEX"] == "F"  # Female -> F
    assert row1["AGE"] == 24
    assert row1["ARM"] == "Placebo"

    # Test VS schema execution
    vs_yaml = schemas["VS"]
    vs_spec = yaml.safe_load(vs_yaml)
    assert vs_spec["domain"] == "VS"
    assert vs_spec["base"] == "ODM"

    vs_engine = CDISCEngine(vs_spec, df_long=parser.df_long)
    vs_df = vs_engine.build()
    assert vs_df.height > 0
    assert "VSTESTCD" in vs_df.columns
    assert "VSSTRESN" in vs_df.columns

def test_ai_generator_single_form_mapping():
    cath_xml = Path("/home/ming/Documents/yamaa/cath/odm/odm.xml")
    if not cath_xml.exists():
        pytest.skip("CATH XML not found")

    parser = ODMParser(cath_xml)
    generator = AISDTMSchemaGenerator(
        metadata_df=parser.get_metadata_summary(),
        df_long=parser.df_long
    )

    # User selects ONLY 1 form
    single_mapping = {"FO.NCT00789880.DM": "DM"}
    schemas = generator.generate_schemas_for_form_mappings(single_mapping)
    
    # Must contain ONLY DM, and no other domains!
    assert len(schemas) == 1
    assert "DM" in schemas
    assert "VS" not in schemas
    assert "AE" not in schemas
