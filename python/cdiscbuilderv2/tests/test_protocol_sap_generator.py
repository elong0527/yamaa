import pytest

from cdiscbuilderv2.study_designs import get_design_by_id, calculate_sample_size, SampleSizeRequest
from cdiscbuilderv2.protocol_sap_generator import ProtocolSAPGenerator
from cdiscbuilderv2.pdf_builder import ClinicalPDFBuilder


def test_protocol_generation_oncology():
    design = get_design_by_id("ONCOLOGY_DOUBLE_BLIND_PFS")
    assert design is not None
    ss_req = SampleSizeRequest(
        design_id="ONCOLOGY_DOUBLE_BLIND_PFS",
        endpoint_type="survival",
        hypothesis="superiority",
        hazard_ratio=0.70,
        alpha=0.05,
        power=0.85
    )
    ss_res = calculate_sample_size(ss_req)
    
    gen = ProtocolSAPGenerator(provider="local")
    res = gen.generate_protocol(design=design, sample_size=ss_res)
    
    assert res["status"] == "SUCCESS"
    assert res["document_type"] == "CLINICAL_TRIAL_PROTOCOL"
    content = res["content"]
    assert "CLINICAL TRIAL PROTOCOL" in content
    assert "PROTOCOL SYNOPSIS" in content
    assert "ICH GCP E6 (R2)" in content
    assert "SCHEDULE OF ACTIVITIES" in content
    assert "STATISTICAL CONSIDERATIONS" in content
    assert str(ss_res["total_enrollment_target"]) in content or str(ss_res["total_n"]) in content


def test_sap_generation_device_non_inferiority():
    design = get_design_by_id("DEVICE_NON_INFERIORITY")
    assert design is not None
    ss_req = SampleSizeRequest(
        design_id="DEVICE_NON_INFERIORITY",
        endpoint_type="binary",
        hypothesis="non_inferiority",
        prop_control=0.85,
        prop_treatment=0.85,
        delta_margin=0.08,
        alpha=0.025,
        power=0.90
    )
    ss_res = calculate_sample_size(ss_req)
    
    gen = ProtocolSAPGenerator(provider="local")
    res = gen.generate_sap(design=design, sample_size=ss_res)
    
    assert res["status"] == "SUCCESS"
    assert res["document_type"] == "STATISTICAL_ANALYSIS_PLAN"
    content = res["content"]
    assert "STATISTICAL ANALYSIS PLAN" in content
    assert "ANALYSIS POPULATIONS & SETS" in content
    assert "Intention-to-Treat" in content
    assert "PRIMARY EFFICACY ENDPOINT ANALYSIS" in content
    assert "TABLE, FIGURE, AND LISTING" in content


def test_pdf_builder_protocol_and_sap():
    design = get_design_by_id("CROSSOVER_BIOEQUIVALENCE")
    assert design is not None
    gen = ProtocolSAPGenerator(provider="local")
    res = gen.generate_protocol(design=design)
    assert res["status"] == "SUCCESS"
    assert "PROTOCOL" in res["content"]

    pdf_builder = ClinicalPDFBuilder()
    pdf_bytes = pdf_builder.build_pdf_from_markdown(
        title="Bioequivalence Study Protocol",
        doc_type="CLINICAL TRIAL PROTOCOL",
        markdown_text=res["content"],
        metadata={
            "protocol_id": "PRT-BE-001",
            "therapeutic_area": "Clinical Pharmacology",
            "phase": "Phase 1"
        }
    )
    assert len(pdf_bytes) > 1000
    assert pdf_bytes[:4] == b"%PDF"

