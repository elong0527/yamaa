import pytest
from cdiscbuilderv2.study_designs import (
    CLINICAL_STUDY_CATALOG,
    SampleSizeRequest,
    calculate_sample_size,
    get_catalog_summary,
    get_design_by_id
)
from cdiscbuilderv2.trial_design_builder import TrialDesignBuilder


def test_study_catalog():
    catalog = get_catalog_summary()
    assert len(catalog) >= 7
    
    # Check Oncology and Medical Device exist
    onco = get_design_by_id("ONCOLOGY_DOUBLE_BLIND_PFS")
    assert onco is not None
    assert "Oncology" in onco["category"]
    assert onco["hypothesis_type"] == "superiority"
    assert onco["endpoint_type"] == "survival"

    device = get_design_by_id("DEVICE_NON_INFERIORITY")
    assert device is not None
    assert "Medical Devices" in device["category"]
    assert device["hypothesis_type"] == "non_inferiority"
    assert device["endpoint_type"] == "binary"


def test_sample_size_continuous_superiority():
    req = SampleSizeRequest(
        design_id="CUSTOM",
        endpoint_type="continuous",
        hypothesis="superiority",
        alpha=0.05,
        power=0.80,
        mean_control=10.0,
        mean_treatment=15.0,
        sd_pooled=10.0,
        allocation_ratio=1.0,
        dropout_rate=0.10
    )
    res = calculate_sample_size(req)
    assert res["total_evaluable"] > 0
    assert res["total_enrollment_target"] > res["total_evaluable"]
    # Standard formula for 1:1, alpha=0.05 (2-sided, z=1.96), power=0.80 (z=0.84), delta=5, sd=10 -> ~63 per arm
    assert 55 <= res["n_control_evaluable"] <= 75


def test_sample_size_medical_device_non_inferiority():
    req = SampleSizeRequest(
        design_id="DEVICE_NON_INFERIORITY",
        endpoint_type="binary",
        hypothesis="non_inferiority",
        alpha=0.025,
        power=0.85,
        prop_control=0.88,
        prop_treatment=0.90,
        delta_margin=0.08,
        allocation_ratio=1.0,
        dropout_rate=0.10
    )
    res = calculate_sample_size(req)
    assert res["total_evaluable"] > 100
    assert res["total_enrollment_target"] > res["total_evaluable"]


def test_sample_size_oncology_survival_logrank():
    req = SampleSizeRequest(
        design_id="ONCOLOGY_DOUBLE_BLIND_PFS",
        endpoint_type="survival",
        hypothesis="superiority",
        alpha=0.05,
        power=0.90,
        hazard_ratio=0.70,
        event_rate_control=0.60,
        allocation_ratio=1.0,
        dropout_rate=0.05
    )
    res = calculate_sample_size(req)
    assert res["events_required"] is not None
    # Schoenfeld for HR=0.70, alpha=0.05 2-sided (1.96), power=0.90 (1.28) -> ~331 events
    assert 300 <= res["events_required"] <= 380
    assert res["total_evaluable"] >= res["events_required"]


def test_trial_design_builder_synthesis():
    builder = TrialDesignBuilder(
        design_id="ONCOLOGY_DOUBLE_BLIND_PFS",
        study_id="ONCO-2026-001"
    )
    schemas = builder.generate_all_tdm_schemas()
    assert "TS" in schemas
    assert "TA" in schemas
    assert "TE" in schemas
    assert "TV" in schemas

    dfs = builder.generate_all_tdm_dataframes()
    assert "TS" in dfs
    ts_df = dfs["TS"]
    assert ts_df.height > 5
    assert "STUDYID" in ts_df.columns
    assert "TSPARMCD" in ts_df.columns
    assert "TSVAL" in ts_df.columns

    ta_df = dfs["TA"]
    assert ta_df.height >= 4
    assert "ARMCD" in ta_df.columns
    assert "EPOCH" in ta_df.columns
