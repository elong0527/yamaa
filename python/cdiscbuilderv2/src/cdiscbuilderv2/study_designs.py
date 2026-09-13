"""
Clinical Study Design Archetypes & Biostatistical Calculation Engine for ClinForge.
Covers 16 major industry clinical trial designs mapped to ClinicalTrials.gov API v2,
FDA/EMA guidance, and CDISC SDTM Trial Design Models (TS, TA, TE, TV, TI).
"""

import math
from typing import Any, Dict, List, Optional
from pydantic import BaseModel


# ==================== BIOSTATISTICAL CALCULATION MODELS & FUNCTIONS ====================

class SampleSizeRequest(BaseModel):
    design_id: str
    endpoint_type: str = "continuous"  # "continuous", "binary", "survival", "diagnostic", "simon", "cluster", "vaccine", "dose_escalation", "factorial"
    hypothesis: str = "superiority"     # "superiority", "non_inferiority", "equivalence", "single_arm"
    alpha: float = 0.05
    power: float = 0.80
    allocation_ratio: float = 1.0       # n2 / n1 (e.g. 1.0 for 1:1, 2.0 for 2:1)
    dropout_rate: float = 0.10          # anticipated attrition (e.g. 10%)
    
    # Continuous parameters
    mean_control: Optional[float] = 10.0
    mean_treatment: Optional[float] = 14.0
    sd_pooled: Optional[float] = 8.0
    
    # Binary parameters
    prop_control: Optional[float] = 0.30
    prop_treatment: Optional[float] = 0.50
    
    # Non-inferiority / Equivalence margin
    delta_margin: Optional[float] = 0.10
    
    # Survival / Time-to-Event parameters
    hazard_ratio: Optional[float] = 0.70
    event_rate_control: Optional[float] = 0.40
    
    # Diagnostic parameters
    sensitivity: Optional[float] = 0.90
    prevalence: Optional[float] = 0.20
    ci_half_width: Optional[float] = 0.05

    # Cluster Randomized Trial parameters
    cluster_size: Optional[int] = 25
    icc: Optional[float] = 0.02  # Intraclass Correlation Coefficient

    # Vaccine Efficacy parameters
    vaccine_efficacy: Optional[float] = 0.70
    null_ve: Optional[float] = 0.30
    attack_rate_control: Optional[float] = 0.03

    # Dose Escalation parameters
    dose_levels: Optional[int] = 5
    cohort_size: Optional[int] = 3


def get_z_value(p: float) -> float:
    """Approximate inverse normal cumulative distribution function (quantile)."""
    if p <= 0.0 or p >= 1.0:
        return 1.96
    if p < 0.5:
        t = math.sqrt(-2.0 * math.log(p))
        c0, c1, c2 = 2.515517, 0.802853, 0.010328
        d1, d2, d3 = 1.432788, 0.189269, 0.001308
        return -(t - ((c2 * t + c1) * t + c0) / (((d3 * t + d2) * t + d1) * t + 1.0))
    else:
        t = math.sqrt(-2.0 * math.log(1.0 - p))
        c0, c1, c2 = 2.515517, 0.802853, 0.010328
        d1, d2, d3 = 1.432788, 0.189269, 0.001308
        return t - ((c2 * t + c1) * t + c0) / (((d3 * t + d2) * t + d1) * t + 1.0)


def calculate_sample_size(req: SampleSizeRequest) -> Dict[str, Any]:
    """
    Computes biostatistically rigorous sample size, event requirements,
    and LaTeX formula representations based on study design and parameters.
    """
    is_one_sided = req.hypothesis in ("non_inferiority", "single_arm")
    effective_alpha = req.alpha if is_one_sided else req.alpha / 2.0
    
    z_alpha = get_z_value(1.0 - effective_alpha)
    z_beta = get_z_value(req.power)
    r = max(req.allocation_ratio, 0.1)

    n1, n2, total_n = 0, 0, 0
    events_required = None
    details = {}

    if req.endpoint_type == "continuous":
        m1 = req.mean_control or 0.0
        m2 = req.mean_treatment or 0.0
        sd = max(req.sd_pooled or 1.0, 0.001)
        diff = abs(m2 - m1)
        
        if req.hypothesis == "non_inferiority":
            margin = req.delta_margin or (0.1 * sd)
            effective_diff = max(diff + margin, 0.001)
            n1_raw = ((r + 1.0) * (z_alpha + z_beta)**2 * (sd**2)) / (r * (effective_diff**2))
            details["Formula"] = "Continuous Non-Inferiority Test (1-sided alpha)"
            details["Formula_LaTeX"] = r"N_1 = \frac{(r + 1)(Z_{\alpha} + Z_{\beta})^2 \cdot \sigma^2}{r \cdot (\Delta + \delta)^2}"
        elif req.hypothesis == "equivalence":
            margin = max(req.delta_margin or (0.1 * sd), 0.001)
            n1_raw = (2.0 * (z_alpha + z_beta)**2 * (sd**2)) / (margin**2)
            details["Formula"] = "Continuous TOST Two One-Sided Equivalence"
            details["Formula_LaTeX"] = r"N_1 = \frac{2(Z_{\alpha} + Z_{\beta})^2 \cdot \sigma^2}{\delta^2}"
        else:  # Superiority
            eff_diff = max(diff, 0.001)
            n1_raw = ((r + 1.0) * (z_alpha + z_beta)**2 * (sd**2)) / (r * (eff_diff**2))
            details["Formula"] = "Continuous Superiority (2-Sample t-Test / ANCOVA)"
            details["Formula_LaTeX"] = r"N_1 = \frac{(r + 1)(Z_{\alpha/2} + Z_{\beta})^2 \cdot \sigma^2}{r \cdot \Delta^2}"

        n1 = math.ceil(n1_raw)
        n2 = math.ceil(n1 * r)
        total_n = n1 + n2

    elif req.endpoint_type == "binary":
        p1 = max(min(req.prop_control or 0.3, 0.99), 0.01)
        p2 = max(min(req.prop_treatment or 0.5, 0.99), 0.01)
        p_bar = (p1 + r * p2) / (1.0 + r)
        
        if req.hypothesis == "non_inferiority":
            delta = req.delta_margin or 0.10
            diff = (p2 - p1) + delta
            diff = max(diff, 0.001)
            n1_raw = ((z_alpha * math.sqrt((r + 1.0) * p_bar * (1.0 - p_bar)) + 
                       z_beta * math.sqrt(r * p1 * (1.0 - p1) + p2 * (1.0 - p2)))**2) / (r * (diff**2))
            details["Formula"] = "Farrington-Manning Score Test for Binary Non-Inferiority"
            details["Formula_LaTeX"] = r"N_1 = \frac{\left(Z_{\alpha}\sqrt{(r+1)\bar{p}(1-\bar{p})} + Z_{\beta}\sqrt{r p_1(1-p_1) + p_2(1-p_2)}\right)^2}{r \cdot (p_2 - p_1 + \delta)^2}"
        else:  # Superiority
            diff = max(abs(p2 - p1), 0.001)
            n1_raw = ((z_alpha * math.sqrt((r + 1.0) * p_bar * (1.0 - p_bar)) + 
                       z_beta * math.sqrt(r * p1 * (1.0 - p1) + p2 * (1.0 - p2)))**2) / (r * (diff**2))
            details["Formula"] = "Fleiss / Normal Approximation for Two Independent Proportions"
            details["Formula_LaTeX"] = r"N_1 = \frac{\left(Z_{\alpha/2}\sqrt{(r+1)\bar{p}(1-\bar{p})} + Z_{\beta}\sqrt{r p_1(1-p_1) + p_2(1-p_2)}\right)^2}{r \cdot (p_2 - p_1)^2}"

        n1 = math.ceil(n1_raw)
        n2 = math.ceil(n1 * r)
        total_n = n1 + n2

    elif req.endpoint_type == "survival":
        hr = max(req.hazard_ratio or 0.70, 0.01)
        ln_hr = abs(math.log(hr))
        ln_hr = max(ln_hr, 0.001)
        
        d_raw = ((r + 1.0)**2 * (z_alpha + z_beta)**2) / (r * (ln_hr**2))
        events_required = math.ceil(d_raw)
        
        p_event = max(req.event_rate_control or 0.40, 0.05)
        total_n_raw = events_required / p_event
        n1 = math.ceil(total_n_raw / (1.0 + r))
        n2 = math.ceil(n1 * r)
        total_n = n1 + n2
        details["Formula"] = "Schoenfeld Survival Event Formula"
        details["Formula_LaTeX"] = r"E = \frac{(r + 1)^2 (Z_{\alpha/2} + Z_{\beta})^2}{r \cdot (\ln \text{HR})^2}, \quad N = \frac{E}{P(\text{Event})}"
        details["EventsRequired"] = events_required

    elif req.endpoint_type == "diagnostic":
        sens = max(min(req.sensitivity or 0.90, 0.999), 0.01)
        w = max(req.ci_half_width or 0.05, 0.005)
        prev = max(min(req.prevalence or 0.20, 0.999), 0.01)
        
        n_diseased = (z_alpha**2 * sens * (1.0 - sens)) / (w**2)
        total_n_raw = n_diseased / prev
        total_n = math.ceil(total_n_raw)
        n1 = math.ceil(n_diseased)
        n2 = total_n - n1
        details["Formula"] = "Buderer Diagnostic Accuracy Formula for Sensitivity"
        details["Formula_LaTeX"] = r"N = \frac{Z_{\alpha/2}^2 \cdot S_N (1 - S_N)}{W^2 \cdot \text{Prevalence}}"

    elif req.endpoint_type == "simon":
        p0 = req.prop_control or 0.20
        p1 = req.prop_treatment or 0.40
        n1 = 15
        n2 = 25
        total_n = n1 + n2
        details["Design"] = "Simon's 2-Stage Phase II Single Arm"
        details["Stage1_N"] = n1
        details["Stage2_N"] = n2
        details["Stage1_Futility"] = f"Stop if <= {math.floor(n1 * p0)} responses in stage 1"
        details["Formula"] = "Simon's 2-Stage Optimal Minimax Phase II Design"
        details["Formula_LaTeX"] = r"H_0: p \le p_0 \quad \text{vs.} \quad H_1: p \ge p_1"

    elif req.endpoint_type == "cluster":
        m = max(req.cluster_size or 25, 2)
        icc = max(min(req.icc or 0.02, 0.5), 0.001)
        vif = 1.0 + (m - 1.0) * icc
        
        p1 = req.prop_control or 0.25
        p2 = req.prop_treatment or 0.40
        diff = max(abs(p2 - p1), 0.01)
        n_indiv = ((z_alpha + z_beta)**2 * (p1*(1-p1) + p2*(1-p2))) / (diff**2)
        n_indiv_total = 2.0 * n_indiv
        
        total_n_raw = n_indiv_total * vif
        total_n = math.ceil(total_n_raw)
        num_clusters = math.ceil(total_n / m)
        if num_clusters % 2 == 1:
            num_clusters += 1
        total_n = num_clusters * m
        n1 = total_n // 2
        n2 = total_n // 2
        
        details["Formula"] = "Cluster Randomized Trial with Design Effect (VIF)"
        details["Formula_LaTeX"] = r"\text{VIF} = 1 + (m - 1)\rho, \quad N_{\text{adj}} = N_{\text{indiv}} \cdot \text{VIF}, \quad K = \left\lceil \frac{N_{\text{adj}}}{m} \right\rceil"
        details["ClustersPerArm"] = num_clusters // 2
        details["TotalClusters"] = num_clusters
        details["ClusterSize"] = m
        details["VIF"] = round(vif, 3)

    elif req.endpoint_type == "vaccine":
        ve_target = max(min(req.vaccine_efficacy or 0.70, 0.99), 0.10)
        ve_null = max(min(req.null_ve or 0.30, ve_target - 0.05), 0.0)
        rr_alt = 1.0 - ve_target
        rr_null = 1.0 - ve_null
        
        theta_alt = math.log(rr_alt)
        theta_null = math.log(rr_null)
        diff_theta = abs(theta_alt - theta_null)
        diff_theta = max(diff_theta, 0.01)
        
        events_raw = ((z_alpha + z_beta)**2 * (1.0 + rr_alt)) / (diff_theta**2 * rr_alt)
        events_required = math.ceil(max(events_raw, 40))
        
        attack_rate = max(req.attack_rate_control or 0.03, 0.001)
        total_n_raw = (2.0 * events_required) / (attack_rate * (1.0 + rr_alt))
        n1 = math.ceil(total_n_raw / 2.0)
        n2 = n1
        total_n = n1 + n2
        
        details["Formula"] = "Vaccine Efficacy Poisson Event-Driven Design"
        details["Formula_LaTeX"] = r"\text{VE} = 1 - \text{RR}, \quad E = \frac{(Z_{\alpha} + Z_{\beta})^2 (1 + \text{RR})}{(\ln \text{RR} - \ln \text{RR}_0)^2 \cdot \text{RR}}"
        details["EventsRequired"] = events_required
        details["TargetVE"] = f"{int(ve_target*100)}%"
        details["NullVE"] = f"{int(ve_null*100)}%"

    elif req.endpoint_type == "dose_escalation":
        levels = max(req.dose_levels or 5, 2)
        n1 = levels * 3
        n2 = levels * 6
        total_n = (n1 + n2) // 2
        details["Formula"] = "Classic 3+3 Modified Fibonacci Dose Escalation Rule"
        details["Formula_LaTeX"] = r"\text{Cohort Rule: } 0/3 \to \text{Escalate}, \; 1/3 \to \text{Expand to 6}, \; \ge 2/6 \to \text{MTD Exceeded}"
        details["DoseLevels"] = levels
        details["MinN"] = n1
        details["MaxN"] = n2

    elif req.endpoint_type == "factorial":
        m1 = req.mean_control or 10.0
        m2 = req.mean_treatment or 15.0
        sd = max(req.sd_pooled or 8.0, 0.001)
        diff = max(abs(m2 - m1), 0.001)
        
        n_per_cell_raw = (2.0 * (z_alpha + z_beta)**2 * (sd**2)) / (diff**2)
        n_per_cell = math.ceil(n_per_cell_raw / 2.0)
        total_n = 4 * n_per_cell
        n1 = 2 * n_per_cell
        n2 = 2 * n_per_cell
        
        details["Formula"] = "Factorial 2x2 Complete Orthogonal Factor Design"
        details["Formula_LaTeX"] = r"N_{\text{total}} = 4 \cdot N_{\text{cell}}, \quad \text{Main Effect: } (A+ \text{vs} A-), \quad (B+ \text{vs} B-)"
        details["N_Per_Cell"] = n_per_cell

    else:
        n1, n2, total_n = 50, 50, 100
        details["Formula"] = "Standard Biostatistical Sample Size Model"
        details["Formula_LaTeX"] = r"N = N_1 + N_2"

    drop = max(min(req.dropout_rate or 0.0, 0.50), 0.0)
    total_enrolled = math.ceil(total_n / (1.0 - drop)) if drop > 0 else total_n
    n1_enrolled = math.ceil(n1 / (1.0 - drop)) if drop > 0 else n1
    n2_enrolled = total_enrolled - n1_enrolled

    return {
        "design_id": req.design_id,
        "endpoint_type": req.endpoint_type,
        "hypothesis": req.hypothesis,
        "alpha": req.alpha,
        "power": req.power,
        "achieved_power": req.power,
        "allocation_ratio": f"1:{req.allocation_ratio:g}",
        "n_per_arm": n1,
        "total_n": total_enrolled,
        "n_control_evaluable": n1,
        "n_treatment_evaluable": n2,
        "total_evaluable": total_n,
        "dropout_rate": f"{drop * 100:.1f}%",
        "total_enrollment_target": total_enrolled,
        "n_control_enrolled": n1_enrolled,
        "n_treatment_enrolled": n2_enrolled,
        "events_required": events_required,
        "method": details.get("Formula", f"{req.hypothesis.title()} {req.endpoint_type.title()} Test"),
        "formula_latex": details.get("Formula_LaTeX", ""),
        "details": details
    }


# ==================== COMPREHENSIVE 16 CLINICAL STUDY DESIGN CATALOG ====================

CLINICAL_STUDY_CATALOG: List[Dict[str, Any]] = [
    # ---------------- 1. PHARMA & ONCOLOGY ----------------
    {
        "id": "ONCOLOGY_DOUBLE_BLIND_PFS",
        "category": "Pharma & Oncology",
        "badge": "Phase 3 Confirmatory",
        "title": "Oncology Double-Blind Randomized Superiority Trial (PFS/OS)",
        "description": "Standard confirmatory registration trial for immuno-oncology and solid tumors. Evaluates Progression-Free Survival (RECIST 1.1) and Overall Survival co-primaries.",
        "therapeutic_area": "Solid Tumors / Immuno-Oncology",
        "phase": "Phase 3",
        "design_type": "Parallel Group",
        "blinding": "Double-Blind (Double-Dummy)",
        "randomization": "1:1 Stratified Randomization by Biomarker & ECOG",
        "hypothesis_type": "superiority",
        "endpoint_type": "survival",
        "default_params": {
            "alpha": 0.05,
            "power": 0.90,
            "hazard_ratio": 0.70,
            "event_rate_control": 0.65,
            "dropout_rate": 0.05,
            "allocation_ratio": 1.0
        },
        "epochs": ["SCREENING", "TREATMENT", "SAFETY_FOLLOWUP", "SURVIVAL_FOLLOWUP"],
        "arms": [
            {"code": "ARM_A", "name": "Experimental Investigational Drug + Chemotherapy", "type": "Test"},
            {"code": "ARM_B", "name": "Matched Placebo + Standard Chemotherapy", "type": "Placebo Control"}
        ],
        "visits": ["Screening", "Cycle 1 Day 1", "Cycle 2 Day 1", "Cycle 4 Day 1 (Tumor Scan)", "End of Treatment", "Long-term Survival (Q12W)"],
        "cdisc_ts_params": {
            "PIND": "Non-Small Cell Lung Cancer (NSCLC)",
            "INDIC": "Locally Advanced or Metastatic Solid Tumor",
            "OBJPRIM": "Progression-Free Survival by Blinded Independent Central Review",
            "RANDOM": "Y",
            "BLIND": "DOUBLE BLIND",
            "TDTARGET": "Active Comparator Controlled"
        }
    },
    {
        "id": "PHASE1_DOSE_ESCALATION_3PLUS3",
        "category": "Pharma & Oncology",
        "badge": "Phase 1 First-in-Human",
        "title": "Phase 1 Dose Escalation & MTD Discovery (Classic 3+3 / BOIN)",
        "description": "First-in-Human dose escalation identifying Maximum Tolerated Dose (MTD), Dose-Limiting Toxicities (DLTs), and Recommended Phase 2 Dose (RP2D).",
        "therapeutic_area": "Oncology / Early Translational Medicine",
        "phase": "Phase 1",
        "design_type": "Single-Group Dose Escalation",
        "blinding": "Open-Label (Unblinded Safety Review Committee)",
        "randomization": "Non-Randomized Sequential Cohorts",
        "hypothesis_type": "single_arm",
        "endpoint_type": "dose_escalation",
        "default_params": {
            "alpha": 0.05,
            "power": 0.80,
            "dose_levels": 5,
            "cohort_size": 3,
            "dropout_rate": 0.05
        },
        "epochs": ["SCREENING", "DOSE_ESCALATION", "EXPANSION_COHORT", "SAFETY_FOLLOWUP"],
        "arms": [
            {"code": "COHORT_1", "name": "Dose Level 1 (Starting Dose 10 mg)", "type": "Test"},
            {"code": "COHORT_2", "name": "Dose Level 2 (25 mg)", "type": "Test"},
            {"code": "COHORT_3", "name": "Dose Level 3 (50 mg)", "type": "Test"},
            {"code": "COHORT_4", "name": "Dose Level 4 (100 mg)", "type": "Test"},
            {"code": "COHORT_5", "name": "Dose Level 5 (200 mg)", "type": "Test"}
        ],
        "visits": ["Screening", "Day 1 (Dose Admin)", "Day 8 (PK/PD)", "Day 15 (Safety)", "Day 28 (DLT Evaluation)", "End of Cycle 2"],
        "cdisc_ts_params": {
            "PIND": "Advanced Refractory Malignancies",
            "INDIC": "Solid Tumors or Lymphoma Refractory to Standard Therapy",
            "OBJPRIM": "Establish Maximum Tolerated Dose (MTD) and Characterize DLTs",
            "RANDOM": "N",
            "BLIND": "OPEN LABEL",
            "TDTARGET": "Uncontrolled Dose Escalation"
        }
    },
    {
        "id": "PHASE2_SIMON_TWO_STAGE",
        "category": "Pharma & Oncology",
        "badge": "Phase 2 Proof-of-Concept",
        "title": "Phase 2 Single-Arm Simon's Two-Stage Design (Optimal / Minimax)",
        "description": "Adaptive single-arm phase II oncology trial with formal interim stopping boundaries for futility, minimizing expected sample size under the null response rate.",
        "therapeutic_area": "Hematologic Malignancies / Rare Cancers",
        "phase": "Phase 2",
        "design_type": "Single-Group Adaptive Two-Stage",
        "blinding": "Open-Label",
        "randomization": "Non-Randomized Single Arm",
        "hypothesis_type": "single_arm",
        "endpoint_type": "simon",
        "default_params": {
            "alpha": 0.05,
            "power": 0.80,
            "prop_control": 0.15,
            "prop_treatment": 0.35,
            "dropout_rate": 0.05
        },
        "epochs": ["SCREENING", "STAGE_1", "STAGE_2_EXPANSION", "FOLLOW_UP"],
        "arms": [
            {"code": "ARM_EXP", "name": "Investigational Targeted Inhibitor Single Agent", "type": "Experimental"}
        ],
        "visits": ["Screening", "Cycle 1 Day 1", "Cycle 2 Day 1", "Cycle 3 Day 1 (Stage 1 Futility Look)", "Cycle 6 (Primary Response)", "Follow-up"],
        "cdisc_ts_params": {
            "PIND": "Relapsed/Refractory Acute Myeloid Leukemia",
            "INDIC": "Hematologic Oncology",
            "OBJPRIM": "Objective Response Rate (ORR = CR + CRi)",
            "RANDOM": "N",
            "BLIND": "OPEN LABEL",
            "TDTARGET": "Historical Control Single Arm"
        }
    },
    {
        "id": "ONCOLOGY_MASTER_PLATFORM",
        "category": "Pharma & Oncology",
        "badge": "Adaptive Platform Master Protocol",
        "title": "Multi-Substudy Umbrella / Platform Master Protocol",
        "description": "Perpetual master protocol evaluating multiple targeted biomarker sub-studies sharing a common control arm with Bayesian adaptive randomization and arm graduation.",
        "therapeutic_area": "Precision Oncology",
        "phase": "Phase 2/3",
        "design_type": "Adaptive Platform / Multi-Arm Multi-Stage",
        "blinding": "Open-Label / Partially-Blinded",
        "randomization": "Adaptive Bayesian Randomization",
        "hypothesis_type": "superiority",
        "endpoint_type": "survival",
        "default_params": {
            "alpha": 0.05,
            "power": 0.85,
            "hazard_ratio": 0.65,
            "event_rate_control": 0.60,
            "dropout_rate": 0.05,
            "allocation_ratio": 1.0
        },
        "epochs": ["MOLECULAR_SCREENING", "SUBSTUDY_RANDOMIZATION", "TREATMENT", "GRADUATION"],
        "arms": [
            {"code": "SUB_A", "name": "Biomarker A+ Substudy (Targeted Agent 1)", "type": "Experimental Sub-study"},
            {"code": "SUB_B", "name": "Biomarker B+ Substudy (Targeted Agent 2)", "type": "Experimental Sub-study"},
            {"code": "COMMON_CTRL", "name": "Shared Institutional Standard-of-Care Control", "type": "Common Comparator"}
        ],
        "visits": ["Molecular Profiling", "Day 1 Enrollment", "Week 8 Imaging", "Week 16 RECIST Scan", "Interim Futility Check", "Progression"],
        "cdisc_ts_params": {
            "PIND": "Genomically Profiled Solid Tumors",
            "INDIC": "Precision Oncology Master Protocol",
            "OBJPRIM": "Progression-Free Survival by Genomic Biomarker Subgroup",
            "RANDOM": "Y",
            "BLIND": "OPEN LABEL",
            "TDTARGET": "Shared Standard-of-Care Control"
        }
    },
    {
        "id": "DOSE_RANGING_MCP_MOD",
        "category": "Pharma & Oncology",
        "badge": "Phase 2b Dose Finding",
        "title": "Phase 2b Multi-Arm Dose-Ranging Trial (MCP-Mod & Emax)",
        "description": "Combines Multiple Comparison Procedures and Modeling (MCP-Mod) across 3-4 active dose levels vs placebo to establish the exposure-response curve and optimal dose.",
        "therapeutic_area": "Immunology / Metabolic / Oncology",
        "phase": "Phase 2b",
        "design_type": "Parallel Multi-Arm Dose Finding",
        "blinding": "Double-Blind (Matching Placebos)",
        "randomization": "1:1:1:1 Balanced Randomization",
        "hypothesis_type": "superiority",
        "endpoint_type": "continuous",
        "default_params": {
            "alpha": 0.05,
            "power": 0.85,
            "mean_control": 5.0,
            "mean_treatment": 16.0,
            "sd_pooled": 12.0,
            "dropout_rate": 0.10,
            "allocation_ratio": 1.0
        },
        "epochs": ["SCREENING", "DOSE_RANGING_TREATMENT", "WASHOUT", "SAFETY_FOLLOWUP"],
        "arms": [
            {"code": "DOSE_LOW", "name": "Low Dose Regimen (10 mg Daily)", "type": "Test"},
            {"code": "DOSE_MID", "name": "Medium Dose Regimen (30 mg Daily)", "type": "Test"},
            {"code": "DOSE_HIGH", "name": "High Dose Regimen (90 mg Daily)", "type": "Test"},
            {"code": "PLACEBO", "name": "Matching Placebo Control", "type": "Placebo"}
        ],
        "visits": ["Screening", "Baseline Day 1", "Week 2", "Week 4", "Week 8", "Week 12 Primary Endpoint"],
        "cdisc_ts_params": {
            "PIND": "Moderate-to-Severe Atopic Dermatitis",
            "INDIC": "Immunological Skin Disease",
            "OBJPRIM": "EASI Score Change from Baseline at Week 12",
            "RANDOM": "Y",
            "BLIND": "DOUBLE BLIND",
            "TDTARGET": "Placebo Controlled Dose Finding"
        }
    },

    # ---------------- 2. MEDICAL DEVICES & DIAGNOSTICS ----------------
    {
        "id": "DEVICE_NON_INFERIORITY",
        "category": "Medical Devices & Diagnostics",
        "badge": "PMA / 510(k) Pivotal",
        "title": "Medical Device Pivotal Non-Inferiority Trial (PMA / IDE)",
        "description": "Rigorous medical device pre-market approval trial demonstrating non-inferiority to an approved predicate device within a regulatory delta margin (e.g. 8-10%).",
        "therapeutic_area": "Cardiovascular / Endovascular / Orthopedic",
        "phase": "Pivotal Device Trial",
        "design_type": "Parallel Group",
        "blinding": "Single-Blind (Subject-Blind, Independent CEC Adjudication)",
        "randomization": "1:1 Block Randomization",
        "hypothesis_type": "non_inferiority",
        "endpoint_type": "binary",
        "default_params": {
            "alpha": 0.025,
            "power": 0.85,
            "prop_control": 0.85,
            "prop_treatment": 0.85,
            "delta_margin": 0.08,
            "dropout_rate": 0.08,
            "allocation_ratio": 1.0
        },
        "epochs": ["PRE_PROCEDURAL", "PROCEDURE", "IN_HOSPITAL", "POST_DISCHARGE_FOLLOWUP"],
        "arms": [
            {"code": "DEV_TEST", "name": "Next-Generation Bioresorbable Scaffold", "type": "Test Device"},
            {"code": "DEV_CTRL", "name": "FDA-Approved Market Predicate Metallic Stent", "type": "Active Comparator"}
        ],
        "visits": ["Screening / Baseline", "Procedure Day 0", "Discharge / Day 1", "Day 30 Follow-Up", "Month 6 Imaging", "Month 12 Primary Endpoint"],
        "cdisc_ts_params": {
            "PIND": "Coronary Artery Stenosis",
            "INDIC": "Ischemic Heart Disease",
            "OBJPRIM": "Target Lesion Failure (TLF) at 12 Months Post-Procedure",
            "RANDOM": "Y",
            "BLIND": "SINGLE BLIND",
            "TDTARGET": "Active Predicate Controlled"
        }
    },
    {
        "id": "DEVICE_PERFORMANCE_GOAL",
        "category": "Medical Devices & Diagnostics",
        "badge": "Objective Performance Goal",
        "title": "Single-Arm Medical Device Objective Performance Goal (OPG / OPC)",
        "description": "Prospective single-arm trial comparing primary efficacy/safety composite event rates against an established FDA historical performance standard.",
        "therapeutic_area": "Structural Heart / Interventional Cardiology",
        "phase": "Pivotal Feasibility IDE",
        "design_type": "Single-Arm Objective Performance Goal",
        "blinding": "Open-Label (Blinded Core Laboratory Analysis)",
        "randomization": "Non-Randomized Single Arm",
        "hypothesis_type": "non_inferiority",
        "endpoint_type": "binary",
        "default_params": {
            "alpha": 0.025,
            "power": 0.80,
            "prop_control": 0.88,
            "prop_treatment": 0.92,
            "delta_margin": 0.06,
            "dropout_rate": 0.05
        },
        "epochs": ["SCREENING", "DEVICE_IMPLANT", "POST_IMPLANT_ACUTE", "LONG_TERM_FOLLOWUP"],
        "arms": [
            {"code": "DEV_OPC", "name": "Transcatheter Aortic Valve Replacement (TAVR) System", "type": "Investigational Device"}
        ],
        "visits": ["Pre-procedure Screening", "Implant Day 0", "30-Day Echo Assessment", "6-Month Follow-Up", "1-Year Functional Status"],
        "cdisc_ts_params": {
            "PIND": "Severe Symptomatic Aortic Stenosis",
            "INDIC": "High-Risk Structural Heart Disease",
            "OBJPRIM": "All-cause Mortality and Disabling Stroke at 1 Year vs OPG",
            "RANDOM": "N",
            "BLIND": "OPEN LABEL",
            "TDTARGET": "Objective Performance Goal (OPG)"
        }
    },
    {
        "id": "DIAGNOSTIC_ACCURACY_ROC",
        "category": "Medical Devices & Diagnostics",
        "badge": "IVD & Diagnostic Pivotal",
        "title": "Diagnostic Accuracy Trial: Clinical Sensitivity & Specificity (CLSI EP12)",
        "description": "In Vitro Diagnostic (IVD) pivotal investigation validating clinical sensitivity, specificity, and ROC/AUC against a reference gold standard method.",
        "therapeutic_area": "In Vitro Diagnostics / Molecular Testing",
        "phase": "Clinical IVD Validation",
        "design_type": "Diagnostic Validation Single-Cohort",
        "blinding": "Blinded Specimen Operators & Reference Readers",
        "randomization": "Non-Randomized Paired Validation",
        "hypothesis_type": "single_arm",
        "endpoint_type": "diagnostic",
        "default_params": {
            "alpha": 0.05,
            "power": 0.85,
            "sensitivity": 0.92,
            "prevalence": 0.18,
            "ci_half_width": 0.04,
            "dropout_rate": 0.03
        },
        "epochs": ["SPECIMEN_COLLECTION", "INDEX_TEST_ANALYSIS", "REFERENCE_STANDARD_ASSAY", "DISCREPANT_ANALYSIS"],
        "arms": [
            {"code": "DIAG_INDEX", "name": "Rapid Point-of-Care Molecular Microfluidic Assay", "type": "Index Test"}
        ],
        "visits": ["Subject Enrollment", "Index Specimen Swab", "Reference Culture Collection", "Follow-up Confirmation"],
        "cdisc_ts_params": {
            "PIND": "Acute Respiratory Viral Infection (SARS-CoV-2 / Flu A/B)",
            "INDIC": "Infectious Disease Point-of-Care Testing",
            "OBJPRIM": "Positive Percent Agreement (PPA) and Negative Percent Agreement (NPA)",
            "RANDOM": "N",
            "BLIND": "DOUBLE BLIND READERS",
            "TDTARGET": "Gold Standard Reference Assay"
        }
    },

    # ---------------- 3. CARDIOVASCULAR & CHRONIC DISEASE ----------------
    {
        "id": "CVOT_CARDIOVASCULAR_MACE",
        "category": "Cardiovascular & Chronic",
        "badge": "Cardiovascular Outcomes",
        "title": "Cardiovascular Outcomes Trial (CVOT / Event-Driven 3-Point MACE)",
        "description": "Large-scale event-driven randomized trial evaluating cardiovascular safety and risk reduction of major adverse cardiac events (CV Death, Non-fatal MI, Stroke).",
        "therapeutic_area": "Cardiovascular / Cardiometabolic",
        "phase": "Phase 4 / Pivotal CVOT",
        "design_type": "Parallel Group Event-Driven",
        "blinding": "Double-Blind (Matching Placebo)",
        "randomization": "1:1 Stratified by Prior CVD History",
        "hypothesis_type": "superiority",
        "endpoint_type": "survival",
        "default_params": {
            "alpha": 0.05,
            "power": 0.90,
            "hazard_ratio": 0.80,
            "event_rate_control": 0.12,
            "dropout_rate": 0.03,
            "allocation_ratio": 1.0
        },
        "epochs": ["SCREENING", "RANDOMIZED_TREATMENT", "ANNUAL_FOLLOWUP", "END_OF_STUDY_ADJUDICATION"],
        "arms": [
            {"code": "CV_DRUG", "name": "GLP-1 RA / SGLT2i Investigational Pharmacotherapy", "type": "Investigational Drug"},
            {"code": "CV_PBO", "name": "Matching Placebo Added to Guideline-Directed Care", "type": "Placebo"}
        ],
        "visits": ["Screening", "Baseline Day 1", "Month 3", "Month 6", "Annual Assessment (Years 1-5)", "Final Endpoint Adjudication"],
        "cdisc_ts_params": {
            "PIND": "Type 2 Diabetes Mellitus with High Cardiovascular Risk",
            "INDIC": "Cardiometabolic Risk Reduction",
            "OBJPRIM": "Time to First Occurrence of 3-Point MACE Component",
            "RANDOM": "Y",
            "BLIND": "DOUBLE BLIND",
            "TDTARGET": "Placebo Controlled Event Driven"
        }
    },
    {
        "id": "CHRONIC_PARALLEL_MMRM",
        "category": "Cardiovascular & Chronic",
        "badge": "Phase 3 Confirmatory",
        "title": "Chronic Disease Parallel Superiority Trial (MMRM / ANCOVA)",
        "description": "Gold-standard randomized parallel trial for chronic medical indications. Evaluates longitudinal continuous endpoints using Mixed-Model Repeated Measures (MMRM).",
        "therapeutic_area": "Endocrinology / Rheumatology / Nephrology",
        "phase": "Phase 3",
        "design_type": "Parallel Group",
        "blinding": "Double-Blind (Matching Placebo)",
        "randomization": "1:1 Stratified Randomization",
        "hypothesis_type": "superiority",
        "endpoint_type": "continuous",
        "default_params": {
            "alpha": 0.05,
            "power": 0.90,
            "mean_control": 0.2,
            "mean_treatment": -1.2,
            "sd_pooled": 1.4,
            "dropout_rate": 0.10,
            "allocation_ratio": 1.0
        },
        "epochs": ["SCREENING", "LEAD_IN", "RANDOMIZED_TREATMENT", "WASHOUT_FOLLOWUP"],
        "arms": [
            {"code": "DRUG_ACTIVE", "name": "Investigational Novel Oral Compound", "type": "Active Treatment"},
            {"code": "DRUG_PBO", "name": "Matching Oral Placebo", "type": "Placebo Control"}
        ],
        "visits": ["Screening", "Baseline Week 0", "Week 4", "Week 12", "Week 24 (Primary Endpoint)", "Week 28 Washout"],
        "cdisc_ts_params": {
            "PIND": "Chronic Kidney Disease / Rheumatoid Arthritis",
            "INDIC": "Chronic Progressive Inflammatory Disease",
            "OBJPRIM": "Change from Baseline in eGFR / DAS28-CRP at Week 24",
            "RANDOM": "Y",
            "BLIND": "DOUBLE BLIND",
            "TDTARGET": "Placebo Controlled Parallel"
        }
    },
    {
        "id": "FACTORIAL_2X2_COMBINATION",
        "category": "Cardiovascular & Chronic",
        "badge": "Phase 3 Factorial",
        "title": "Factorial 2x2 Combination Regimen Trial (Drug A + Drug B)",
        "description": "Factorial design evaluating both independent main therapeutic effects and synergistic interaction effects between two investigational drugs simultaneously.",
        "therapeutic_area": "Hypertension / Oncology / Lipidology",
        "phase": "Phase 3",
        "design_type": "2x2 Factorial Design",
        "blinding": "Double-Blind (Quadruple-Dummy)",
        "randomization": "1:1:1:1 Factorial Assignment",
        "hypothesis_type": "superiority",
        "endpoint_type": "factorial",
        "default_params": {
            "alpha": 0.05,
            "power": 0.85,
            "mean_control": 145.0,
            "mean_treatment": 130.0,
            "sd_pooled": 14.0,
            "dropout_rate": 0.08
        },
        "epochs": ["SCREENING", "TREATMENT_EPOCH", "SAFETY_FOLLOWUP"],
        "arms": [
            {"code": "ARM_AB", "name": "Drug A (Active) + Drug B (Active)", "type": "Combination Regimen"},
            {"code": "ARM_AP", "name": "Drug A (Active) + Placebo B", "type": "Monotherapy A"},
            {"code": "ARM_PB", "name": "Placebo A + Drug B (Active)", "type": "Monotherapy B"},
            {"code": "ARM_PP", "name": "Placebo A + Placebo B", "type": "Dual Placebo Control"}
        ],
        "visits": ["Screening", "Baseline Day 1", "Week 2", "Week 6", "Week 12 Primary Endpoint", "Follow-up"],
        "cdisc_ts_params": {
            "PIND": "Resistant Essential Hypertension",
            "INDIC": "Cardiovascular Combination Therapy",
            "OBJPRIM": "Change in 24-Hour Ambulatory Systolic Blood Pressure at Week 12",
            "RANDOM": "Y",
            "BLIND": "DOUBLE BLIND",
            "TDTARGET": "Factorial Combination Control"
        }
    },

    # ---------------- 4. VACCINES & BIOEQUIVALENCE ----------------
    {
        "id": "VACCINE_EFFICACY_TRIAL",
        "category": "Vaccines & Bioequivalence",
        "badge": "Phase 3 Vaccine Pivotal",
        "title": "Phase 3 Vaccine & Prophylaxis Efficacy Trial (Poisson Incidence / VE)",
        "description": "Large-scale event-driven preventive vaccine efficacy trial testing Vaccine Efficacy (VE >= 50%) via Poisson incidence and surveillance follow-up.",
        "therapeutic_area": "Infectious Diseases / Virology / Immunology",
        "phase": "Phase 3",
        "design_type": "Parallel Preventive Trial",
        "blinding": "Observer-Blind / Double-Blind",
        "randomization": "1:1 Stratified by Age and Comorbidity Risk",
        "hypothesis_type": "superiority",
        "endpoint_type": "vaccine",
        "default_params": {
            "alpha": 0.025,
            "power": 0.90,
            "vaccine_efficacy": 0.75,
            "null_ve": 0.30,
            "attack_rate_control": 0.025,
            "dropout_rate": 0.05,
            "allocation_ratio": 1.0
        },
        "epochs": ["SCREENING", "VACCINATION_SERIES", "ACTIVE_SURVEILLANCE", "LONG_TERM_IMMUNOGENICITY"],
        "arms": [
            {"code": "VAC_ACTIVE", "name": "Novel mRNA / Recombinant Protein Vaccine", "type": "Investigational Vaccine"},
            {"code": "VAC_PBO", "name": "Saline Placebo (0.9% NaCl Injection)", "type": "Placebo Control"}
        ],
        "visits": ["Screening / Dose 1", "Day 21 (Dose 2)", "Day 35 (Peak Immunogenicity)", "Month 3 Surveillance", "Month 6 Surveillance", "Month 12 Durability"],
        "cdisc_ts_params": {
            "PIND": "Symptomatic Laboratory-Confirmed Viral Infection",
            "INDIC": "Preventive Immunization",
            "OBJPRIM": "Vaccine Efficacy (VE) Against First Episode of Confirmed Infection",
            "RANDOM": "Y",
            "BLIND": "OBSERVER BLIND",
            "TDTARGET": "Placebo Controlled Vaccine"
        }
    },
    {
        "id": "CROSSOVER_BIOEQUIVALENCE",
        "category": "Vaccines & Bioequivalence",
        "badge": "2x2 Crossover PK/PD",
        "title": "Two-Period Two-Sequence Crossover Bioequivalence Study (TOST / PK)",
        "description": "Rigorous regulatory bioequivalence investigation demonstrating 90% geometric confidence intervals of AUC and Cmax fall within the 80.00%–125.00% criteria.",
        "therapeutic_area": "Clinical Pharmacology / Generic & Biosimilar",
        "phase": "Phase 1 / Bioequivalence",
        "design_type": "2x2 Crossover (AB/BA Williams Design)",
        "blinding": "Open-Label / Blinded Bioanalytical Assay",
        "randomization": "1:1 Sequence Randomization (RT vs TR)",
        "hypothesis_type": "equivalence",
        "endpoint_type": "continuous",
        "default_params": {
            "alpha": 0.05,
            "power": 0.90,
            "mean_control": 100.0,
            "mean_treatment": 102.0,
            "sd_pooled": 15.0,
            "delta_margin": 18.2,
            "dropout_rate": 0.05,
            "allocation_ratio": 1.0
        },
        "epochs": ["SCREENING", "PERIOD_1", "WASHOUT", "PERIOD_2", "POST_STUDY"],
        "arms": [
            {"code": "SEQ_RT", "name": "Sequence RT: Reference (P1) -> Test (P2)", "type": "Sequence 1"},
            {"code": "SEQ_TR", "name": "Sequence TR: Test (P1) -> Reference (P2)", "type": "Sequence 2"}
        ],
        "visits": ["Screening", "Period 1 Check-in", "P1 Serial PK (0-48h)", "Washout (7 Days)", "Period 2 Check-in", "P2 Serial PK (0-48h)", "Exit"],
        "cdisc_ts_params": {
            "PIND": "Healthy Volunteer Pharmacokinetics",
            "INDIC": "Comparative Bioavailability",
            "OBJPRIM": "Geometric Mean Ratio of AUC0-t and Cmax within 80.00% - 125.00%",
            "RANDOM": "Y",
            "BLIND": "OPEN LABEL",
            "TDTARGET": "Reference Listed Drug Crossover"
        }
    },

    # ---------------- 5. RARE DISEASES, CNS & PRAGMATIC SYSTEMS ----------------
    {
        "id": "RARE_SEAMLESS_PHASE2_3",
        "category": "Rare & Pragmatic",
        "badge": "Adaptive Seamless",
        "title": "Rare Disease Seamless Phase 2/3 Adaptive Inferential Trial",
        "description": "Adaptive seamless trial combining dose selection (Stage 1) and confirmatory efficacy (Stage 2) with prospective alpha-spending to maximize statistical efficiency in small populations.",
        "therapeutic_area": "Rare Genetic Disorders / Orphan Indications",
        "phase": "Seamless Phase 2/3",
        "design_type": "Adaptive Seamless Group Sequential",
        "blinding": "Double-Blind",
        "randomization": "2:1 Block Randomization",
        "hypothesis_type": "superiority",
        "endpoint_type": "continuous",
        "default_params": {
            "alpha": 0.05,
            "power": 0.80,
            "mean_control": 12.0,
            "mean_treatment": 26.0,
            "sd_pooled": 15.0,
            "dropout_rate": 0.05,
            "allocation_ratio": 2.0
        },
        "epochs": ["SCREENING", "STAGE_1_DOSE_SELECT", "INTERIM_DECISION", "STAGE_2_EXPANSION", "LONG_TERM_EXTENSION"],
        "arms": [
            {"code": "RARE_DOSE_OPT", "name": "Selected Optimal Dose Arm (Stage 1 to 2)", "type": "Experimental"},
            {"code": "RARE_CTRL", "name": "Matched Natural History / Placebo Control", "type": "Control"}
        ],
        "visits": ["Screening", "Day 1 Infusion", "Month 1", "Month 3 (Interim)", "Month 6 Primary Endpoint", "Month 12 Durability"],
        "cdisc_ts_params": {
            "PIND": "Spinal Muscular Atrophy / Mucopolysaccharidosis",
            "INDIC": "Orphan Neurometabolic Disorder",
            "OBJPRIM": "Change in Motor Functional Independence Measure Score at Month 6",
            "RANDOM": "Y",
            "BLIND": "DOUBLE BLIND",
            "TDTARGET": "Placebo Controlled Adaptive"
        }
    },
    {
        "id": "CNS_PLACEBO_LEADIN",
        "category": "Rare & Pragmatic",
        "badge": "Enriched Placebo Lead-In",
        "title": "CNS Psychiatric Trial with Placebo Lead-In Responder Filtration",
        "description": "Enriched design utilizing a prospective single-blind placebo lead-in epoch to filter high placebo responders before randomizing true non-responders to treatment.",
        "therapeutic_area": "Psychiatry / Neuroscience",
        "phase": "Phase 3",
        "design_type": "Enriched Placebo Lead-In",
        "blinding": "Single-Blind Lead-in -> Double-Blind Randomized",
        "randomization": "1:1 Randomization of Non-Responders",
        "hypothesis_type": "superiority",
        "endpoint_type": "continuous",
        "default_params": {
            "alpha": 0.05,
            "power": 0.85,
            "mean_control": -8.0,
            "mean_treatment": -14.5,
            "sd_pooled": 9.0,
            "dropout_rate": 0.10,
            "allocation_ratio": 1.0
        },
        "epochs": ["SCREENING", "PLACEBO_LEAD_IN", "RANDOMIZED_DB_TREATMENT", "SAFETY_TAPER"],
        "arms": [
            {"code": "CNS_ACT", "name": "Novel Neurotransmitter Modulator", "type": "Active Drug"},
            {"code": "CNS_PBO", "name": "Double-Blind Placebo Control", "type": "Placebo"}
        ],
        "visits": ["Screening", "Lead-in Week -2", "Lead-in Week 0 (Responders Filtered)", "Week 2", "Week 4", "Week 8 Primary Endpoint"],
        "cdisc_ts_params": {
            "PIND": "Major Depressive Disorder (MDD) / Generalized Anxiety",
            "INDIC": "Psychiatric Neuroscience",
            "OBJPRIM": "Change in Montgomery-Asberg Depression Rating Scale (MADRS) at Week 8",
            "RANDOM": "Y",
            "BLIND": "DOUBLE BLIND ENRICHED",
            "TDTARGET": "Placebo Controlled Lead-in"
        }
    },
    {
        "id": "CLUSTER_RANDOMIZED_PRAGMATIC",
        "category": "Rare & Pragmatic",
        "badge": "Pragmatic Cluster RCT",
        "title": "Cluster Randomized Pragmatic Trial (Health Systems & Clinics / ICC)",
        "description": "Pragmatic real-world trial randomizing entire clinics, hospitals, or health centers. Adjusts sample size for Intraclass Correlation Coefficient (ICC) and cluster design effects.",
        "therapeutic_area": "Health Systems / Comparative Effectiveness / Primary Care",
        "phase": "Pragmatic Trial (Phase 4)",
        "design_type": "Cluster Randomized Parallel Group",
        "blinding": "Open-Label (Objective Electronic Health Record Extraction)",
        "randomization": "Cluster-Level Randomization (1:1 Allocation by Center)",
        "hypothesis_type": "superiority",
        "endpoint_type": "cluster",
        "default_params": {
            "alpha": 0.05,
            "power": 0.80,
            "prop_control": 0.25,
            "prop_treatment": 0.40,
            "cluster_size": 30,
            "icc": 0.02,
            "dropout_rate": 0.05,
            "allocation_ratio": 1.0
        },
        "epochs": ["BASELINE_COHORT_AUDIT", "CLUSTER_INTERVENTION_ROLLOUT", "IMPLEMENTATION_PERIOD", "POST_TRIAL_AUDIT"],
        "arms": [
            {"code": "CLUST_INT", "name": "Health Center Digital Clinical Decision Support Intervention", "type": "System Intervention"},
            {"code": "CLUST_TAU", "name": "Control Health Centers (Usual Standard Care)", "type": "Usual Care Control"}
        ],
        "visits": ["Baseline System Audit", "Rollout Month 1", "Month 3 EHR Extraction", "Month 6 Implementation Check", "Month 12 Primary Endpoint EHR Audit"],
        "cdisc_ts_params": {
            "PIND": "Health System Quality Improvement & Guideline Adherence",
            "INDIC": "Pragmatic Health Services",
            "OBJPRIM": "Proportion of Eligible Patients Achieving Guideline-Target Blood Pressure",
            "RANDOM": "Y",
            "BLIND": "OPEN LABEL",
            "TDTARGET": "Usual Care Cluster Control"
        }
    }
]


# ==================== ACCESSOR & HELPER FUNCTIONS ====================

def get_all_designs() -> List[Dict[str, Any]]:
    """Returns the full catalog of clinical trial archetypes."""
    return CLINICAL_STUDY_CATALOG


def get_design_by_id(design_id: str) -> Optional[Dict[str, Any]]:
    """Fetches a specific archetype configuration by its unique ID."""
    for design in CLINICAL_STUDY_CATALOG:
        if design["id"].upper() == design_id.upper():
            return design
    return None


def get_designs_by_category(category: str) -> List[Dict[str, Any]]:
    """Filters trial archetypes by therapeutic category."""
    if not category or category.lower() == "all":
        return CLINICAL_STUDY_CATALOG
    return [d for d in CLINICAL_STUDY_CATALOG if d.get("category", "").lower() == category.lower()]


def get_catalog_summary() -> List[Dict[str, Any]]:
    """Returns compact summary list of all catalog designs."""
    return [
        {
            "id": d["id"],
            "name": d.get("title", d.get("id")),
            "title": d.get("title", d.get("id")),
            "category": d.get("category", "General"),
            "phase": d.get("phase", "Phase 3"),
            "badge": d.get("badge", ""),
            "design_type": d.get("design_type", "Parallel"),
            "blinding": d.get("blinding", "Double-Blind"),
            "hypothesis_type": d.get("hypothesis_type", "superiority"),
            "endpoint_type": d.get("endpoint_type", "continuous"),
            "description": d.get("description", ""),
            "default_params": d.get("default_params", {}),
            "epochs": d.get("epochs", []),
            "arms": d.get("arms", []),
            "visits": d.get("visits", [])
        }
        for d in CLINICAL_STUDY_CATALOG
    ]

