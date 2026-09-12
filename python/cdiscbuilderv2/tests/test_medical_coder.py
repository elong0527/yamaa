"""
Test Suite for Automated Medical Coding Engine (MedDRA & WHO Drug Global).
Validates Exact/Synonym/Fuzzy resolution, Polars DataFrame auto-coding,
and Yamaa YAML engine integration.
"""

import pytest
import yaml
import polars as pl

from cdiscbuilderv2.medical_coder import (
    MEDDRA_CODER,
    WHO_DRUG_CODER,
    normalize_clinical_term,
    levenshtein_similarity,
    auto_code_dataframe,
    code_meddra_term,
    code_whodrug_term
)
from cdiscbuilderv2.engine import CDISCEngine


def test_term_normalization():
    """Verifies dosage and route stripping."""
    assert normalize_clinical_term("Tylenol 500mg PO Q6H") == "tylenol"
    assert normalize_clinical_term("Pembrolizumab 200 mg IV") == "pembrolizumab"
    assert normalize_clinical_term("severe headache!") == "severe headache"


def test_meddra_exact_match():
    """Verifies MedDRA exact match on PT and LLT."""
    rec = MEDDRA_CODER.code_term("Headache")
    assert rec.pt == "Headache"
    assert rec.pt_code == "10019211"
    assert rec.soc == "Nervous system disorders"
    assert rec.match_tier == "EXACT_MATCH"
    assert rec.confidence == 1.0


def test_meddra_synonym_and_substring():
    """Verifies MedDRA resolution on clinical synonyms and descriptive phrases."""
    rec1 = MEDDRA_CODER.code_term("mild fever after infusion")
    assert rec1.pt == "Pyrexia"
    assert rec1.soc == "General disorders and administration site conditions"
    assert rec1.confidence >= 0.90

    rec2 = MEDDRA_CODER.code_term("nausea and vomitting")
    assert rec2.pt in ("Nausea", "Vomiting")
    assert rec2.soc == "Gastrointestinal disorders"

    rec3 = MEDDRA_CODER.code_term("elevated ALT (3x ULN)")
    assert rec3.pt == "Alanine aminotransferase increased"
    assert rec3.soc == "Investigations"


def test_meddra_fuzzy_typo():
    """Verifies Levenshtein fuzzy match on typos."""
    rec = MEDDRA_CODER.code_term("hypertention")
    assert rec.pt == "Hypertension"
    assert rec.soc == "Vascular disorders"
    assert rec.match_tier == "FUZZY_MATCH"
    assert rec.confidence >= 0.85


def test_whodrug_exact_and_synonym():
    """Verifies WHO Drug resolution for brand names and active substances."""
    # Brand to Generic
    rec1 = WHO_DRUG_CODER.code_term("Tylenol 500mg")
    assert rec1.preferred_name == "PARACETAMOL"
    assert rec1.atc_code == "N02BE01"
    assert "NERVOUS SYSTEM" in rec1.atc_level1

    rec2 = WHO_DRUG_CODER.code_term("Advil liquid gels")
    assert rec2.preferred_name == "IBUPROFEN"
    assert rec2.atc_code == "M01AE01"

    rec3 = WHO_DRUG_CODER.code_term("Lipitor 20mg daily")
    assert rec3.preferred_name == "ATORVASTATIN"
    assert rec3.atc_code == "C10AA05"

    rec4 = WHO_DRUG_CODER.code_term("Keytruda 100mg IV")
    assert rec4.preferred_name == "PEMBROLIZUMAB"
    assert "L01FF02" in rec4.atc_code


def test_auto_code_dataframe_ae():
    """Verifies batch DataFrame coding on AE domain."""
    df = pl.DataFrame({
        "USUBJID": ["01", "02", "03"],
        "AETERM": ["severe headache", "nausea", "high blood pressure"]
    })
    enriched, metrics = auto_code_dataframe(df, "AE")
    assert "AEDECOD" in enriched.columns
    assert "AEBODSYS" in enriched.columns
    assert "AEPTCD" in enriched.columns
    assert "AELLT" in enriched.columns
    assert metrics["auto_coded_high_confidence"] == 3
    assert enriched["AEDECOD"].to_list() == ["Headache", "Nausea", "Hypertension"]


def test_auto_code_dataframe_cm():
    """Verifies batch DataFrame coding on CM domain."""
    df = pl.DataFrame({
        "USUBJID": ["01", "02"],
        "CMTRT": ["Tylenol 500mg PO", "Metformin 1000mg"]
    })
    enriched, metrics = auto_code_dataframe(df, "CM")
    assert "CMDECOD" in enriched.columns
    assert "CMCLAS" in enriched.columns
    assert "CMATC" in enriched.columns
    assert metrics["auto_coded_high_confidence"] == 2
    assert enriched["CMDECOD"].to_list() == ["PARACETAMOL", "METFORMIN"]


def test_transformation_engine_meddra_integration():
    """Verifies YAML pipeline executes meddra_decode and meddra_soc expressions."""
    raw_ae = pl.DataFrame({
        "USUBJID": ["01-001", "01-002"],
        "AETERM": ["fatigue", "diarrhea"]
    })

    schema_yaml = """
domain: AE
base: RAW
datasets:
  RAW: memory
columns:
  - name: USUBJID
    type: str
    derivation: { source: USUBJID }
  - name: AETERM
    type: str
    derivation: { source: AETERM }
  - name: AEDECOD
    type: str
    derivation: { meddra_decode: { source: AETERM } }
  - name: AEBODSYS
    type: str
    derivation: { meddra_soc: { source: AETERM } }
"""
    engine = CDISCEngine(spec=yaml.safe_load(schema_yaml), datasets={"RAW": raw_ae})
    sdtm_ae = engine.build()
    assert sdtm_ae["AEDECOD"].to_list() == ["Fatigue", "Diarrhoea"]
    assert sdtm_ae["AEBODSYS"].to_list() == [
        "General disorders and administration site conditions",
        "Gastrointestinal disorders"
    ]
