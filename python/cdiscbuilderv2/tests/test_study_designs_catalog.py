"""
Comprehensive Test Suite for all 16 ClinicalTrials.gov Clinical Study Design Archetypes.
Validates Sample Size & LaTeX formulas, CDISC SDTM TDM Datasets, Protocol/SAP Synthesis, and PDF Generation.
"""

import pytest

from cdiscbuilderv2.study_designs import (
    CLINICAL_STUDY_CATALOG,
    get_all_designs,
    get_design_by_id,
    get_designs_by_category,
    calculate_sample_size,
    SampleSizeRequest
)
from cdiscbuilderv2.trial_design_builder import TrialDesignBuilder
from cdiscbuilderv2.protocol_sap_generator import ProtocolSAPGenerator
from cdiscbuilderv2.pdf_builder import ClinicalPDFBuilder


def test_all_16_archetypes_present():
    """Verifies all 16 clinical design archetypes are loaded in the catalog."""
    designs = get_all_designs()
    assert len(designs) == 16
    
    categories = {d["category"] for d in designs}
    assert "Pharma & Oncology" in categories
    assert "Medical Devices & Diagnostics" in categories
    assert "Cardiovascular & Chronic" in categories
    assert "Vaccines & Bioequivalence" in categories
    assert "Rare & Pragmatic" in categories


def test_category_filtering():
    """Verifies category filtering works for all categories."""
    assert len(get_designs_by_category("All")) == 16
    assert len(get_designs_by_category("Pharma & Oncology")) == 5
    assert len(get_designs_by_category("Medical Devices & Diagnostics")) == 3
    assert len(get_designs_by_category("Cardiovascular & Chronic")) == 3
    assert len(get_designs_by_category("Vaccines & Bioequivalence")) == 2
    assert len(get_designs_by_category("Rare & Pragmatic")) == 3


@pytest.mark.parametrize("design", CLINICAL_STUDY_CATALOG, ids=lambda d: d["id"])
def test_archetype_sample_size_and_tdm(design):
    """Verifies sample size and TDM datasets for each archetype."""
    def_p = design.get("default_params", {})
    req = SampleSizeRequest(
        design_id=design["id"],
        endpoint_type=design["endpoint_type"],
        hypothesis=design["hypothesis_type"],
        **def_p
    )
    ss_res = calculate_sample_size(req)
    assert ss_res["total_enrollment_target"] > 0
    assert "formula_latex" in ss_res
    assert len(ss_res["formula_latex"]) > 0

    # CDISC SDTM Trial Design Domains
    tdm_builder = TrialDesignBuilder(design["id"], study_id="PRT-" + design["id"])
    tdm_dfs = tdm_builder.generate_all_tdm_dataframes()
    assert "TS" in tdm_dfs and len(tdm_dfs["TS"]) > 0
    assert "TA" in tdm_dfs and len(tdm_dfs["TA"]) > 0
    assert "TE" in tdm_dfs and len(tdm_dfs["TE"]) > 0
    assert "TV" in tdm_dfs and len(tdm_dfs["TV"]) > 0


@pytest.mark.parametrize("design", CLINICAL_STUDY_CATALOG, ids=lambda d: d["id"])
def test_archetype_protocol_sap_and_pdf(design):
    """Verifies ICH E6 Protocol, ICH E9 SAP, and PDF generation for each archetype."""
    gen = ProtocolSAPGenerator(provider="local")
    builder = ClinicalPDFBuilder()
    
    def_p = design.get("default_params", {})
    req = SampleSizeRequest(
        design_id=design["id"],
        endpoint_type=design["endpoint_type"],
        hypothesis=design["hypothesis_type"],
        **def_p
    )
    ss_res = calculate_sample_size(req)
    
    prot_res = gen.generate_protocol(design, ss_res)
    sap_res = gen.generate_sap(design, ss_res)
    
    assert prot_res["status"] == "SUCCESS"
    assert "PROTOCOL" in prot_res["content"]
    assert "SCHEDULE OF ACTIVITIES" in prot_res["content"]
    
    assert sap_res["status"] == "SUCCESS"
    assert "STATISTICAL ANALYSIS PLAN" in sap_res["content"]
    assert "ANALYSIS POPULATIONS" in sap_res["content"]
    
    # PDF Compilation
    prot_pdf = builder.build_pdf_from_markdown(
        title=prot_res["title"],
        doc_type="CLINICAL TRIAL PROTOCOL (ICH GCP E6 R2)",
        markdown_text=prot_res["content"],
        metadata={"protocol_id": "PRT-" + design["id"], "phase": design.get("phase", "Phase 3")}
    )
    assert len(prot_pdf) > 2000
    assert prot_pdf[:4] == b"%PDF"


def test_catalog_designs_count():
    """Verifies study design catalog returns all 16 clinical design archetypes."""
    designs = get_all_designs()
    assert len(designs) == 16
