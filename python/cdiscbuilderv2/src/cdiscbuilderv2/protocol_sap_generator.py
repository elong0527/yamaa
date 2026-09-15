"""
AI Clinical Trial Protocol & Statistical Analysis Plan (SAP) Generator.
Produces regulatory-grade, publication-ready Clinical Trial Protocols (ICH GCP E6 R2)
and Statistical Analysis Plans (ICH E9 / E9(R1)) from clinical study design archetypes.
"""

import os
import json
import logging
from typing import Any, Dict, List, Optional
from datetime import datetime

logger = logging.getLogger("cdiscbuilderv2.protocol_sap")


class ProtocolSAPGenerator:
    """
    Synthesizes ICH E6(R2) Clinical Protocols and ICH E9 Statistical Analysis Plans
    using deterministic clinical expert templating and optional LLM augmentation
    (Google Gemini, Anthropic Claude, OpenAI).
    """

    def __init__(
        self,
        provider: str = "auto",
        api_key: Optional[str] = None,
        model_name: Optional[str] = None
    ):
        self.provider = provider.lower() if provider else "auto"
        self.api_key = api_key or os.getenv("GEMINI_API_KEY") or os.getenv("ANTHROPIC_API_KEY") or os.getenv("OPENAI_API_KEY")
        self.model_name = model_name

        if self.provider == "auto":
            if self.api_key:
                if self.api_key.startswith("AIzaSy"):
                    self.provider = "gemini"
                elif self.api_key.startswith("sk-ant-"):
                    self.provider = "anthropic"
                elif self.api_key.startswith("sk-"):
                    self.provider = "openai"
                else:
                    self.provider = "gemini"
            else:
                self.provider = "local"

    def generate_protocol(
        self,
        design: Dict[str, Any],
        sample_size: Optional[Dict[str, Any]] = None,
        custom_prompt: Optional[str] = None
    ) -> Dict[str, Any]:
        """Generates a comprehensive ICH E6 (R2) Clinical Trial Protocol."""
        if self.provider in ("gemini", "anthropic", "openai") and self.api_key:
            try:
                llm_protocol = self._generate_protocol_llm(design, sample_size, custom_prompt)
                if llm_protocol:
                    return {
                        "status": "SUCCESS",
                        "provider": self.provider,
                        "document_type": "CLINICAL_TRIAL_PROTOCOL",
                        "title": f"Protocol: {design.get('title', 'Clinical Trial')}",
                        "content": llm_protocol
                    }
            except Exception as e:
                logger.warning(f"LLM protocol generation failed ({e}), falling back to expert synthesis.")

        # Local Expert Synthesis Engine
        protocol_md = self._synthesize_protocol_expert(design, sample_size)
        return {
            "status": "SUCCESS",
            "provider": "local_expert_synthesis",
            "document_type": "CLINICAL_TRIAL_PROTOCOL",
            "title": f"Protocol: {design.get('title', 'Clinical Trial')}",
            "content": protocol_md
        }

    def generate_sap(
        self,
        design: Dict[str, Any],
        sample_size: Optional[Dict[str, Any]] = None,
        custom_prompt: Optional[str] = None
    ) -> Dict[str, Any]:
        """Generates a comprehensive ICH E9 Statistical Analysis Plan (SAP)."""
        if self.provider in ("gemini", "anthropic", "openai") and self.api_key:
            try:
                llm_sap = self._generate_sap_llm(design, sample_size, custom_prompt)
                if llm_sap:
                    return {
                        "status": "SUCCESS",
                        "provider": self.provider,
                        "document_type": "STATISTICAL_ANALYSIS_PLAN",
                        "title": f"Statistical Analysis Plan: {design.get('title', 'Clinical Trial')}",
                        "content": llm_sap
                    }
            except Exception as e:
                logger.warning(f"LLM SAP generation failed ({e}), falling back to expert synthesis.")

        # Local Expert Synthesis Engine
        sap_md = self._synthesize_sap_expert(design, sample_size)
        return {
            "status": "SUCCESS",
            "provider": "local_expert_synthesis",
            "document_type": "STATISTICAL_ANALYSIS_PLAN",
            "title": f"Statistical Analysis Plan: {design.get('title', 'Clinical Trial')}",
            "content": sap_md
        }

    @staticmethod
    def _build_soa_table(visits: List[str]) -> str:
        """Constructs a clean, valid GitHub Flavored Markdown table for the Schedule of Activities."""
        if not visits:
            visits = ["Screening", "Baseline", "Week 4", "Week 12", "End of Study"]
        n_v = len(visits)
        header = "| Protocol Assessment | " + " | ".join(visits) + " |"
        separator = "|" + "|".join([":---"] * (n_v + 1)) + "|"
        
        def make_row(label: str, marks: List[str]) -> str:
            padded = (marks + [""] * n_v)[:n_v]
            return f"| {label} | " + " | ".join(padded) + " |"
        
        rows = [
            header,
            separator,
            make_row("**Informed Consent**", ["X"] + [""] * (n_v - 1)),
            make_row("**Demographics & Medical History**", ["X"] + [""] * (n_v - 1)),
            make_row("**Inclusion / Exclusion Review**", ["X", "X"] + [""] * max(0, n_v - 2)),
            make_row("**Randomization / Arm Assignment**", ["", "X"] + [""] * max(0, n_v - 2)),
            make_row("**Investigational Product Dosing**", [""] + ["X"] * max(0, n_v - 2) + [""]),
            make_row("**Vital Signs & Physical Exam**", ["X"] * n_v),
            make_row("**Safety Laboratory Panel**", ["X"] * n_v),
            make_row("**Primary Efficacy Assessment**", [""] + ["X" if (i % 2 == 1 or i == n_v - 1) else "" for i in range(1, n_v)]),
            make_row("**Adverse Event Monitoring**", ["X"] * n_v),
            make_row("**Concomitant Medications**", ["X"] * n_v),
        ]
        return "\n".join(rows)

    def _synthesize_protocol_expert(
        self,
        d: Dict[str, Any],
        ss: Optional[Dict[str, Any]] = None
    ) -> str:
        """Constructs an industry-grade ICH E6(R2) Clinical Trial Protocol in Markdown."""
        today = datetime.now().strftime("%B %d, %Y")
        title = d.get("title", "Randomized Controlled Trial")
        category = d.get("category", "General Medicine")
        phase = d.get("phase", "Phase 3")
        therapeutic_area = d.get("therapeutic_area", "Internal Medicine")
        design_type = d.get("design_type", "Parallel Group")
        blinding = d.get("blinding", "Double-Blind")
        randomization = d.get("randomization", "1:1 Stratified Randomization")
        hypothesis = d.get("hypothesis_type", "superiority").replace("_", " ").title()
        endpoint_type = d.get("endpoint_type", "continuous").title()
        
        arms = d.get("arms", [{"name": "Investigational Product", "type": "Test"}, {"name": "Control / Placebo", "type": "Comparator"}])
        epochs = d.get("epochs", ["SCREENING", "TREATMENT", "FOLLOW_UP"])
        visits = d.get("visits", ["Screening", "Baseline", "Week 4", "Week 12", "End of Study"])
        
        n_total = ss.get("total_enrollment_target", ss.get("total_n", 200)) if ss else 200
        n_arm1 = ss.get("n_control_enrolled", ss.get("n_per_arm", 100)) if ss else 100
        n_arm2 = ss.get("n_treatment_enrolled", ss.get("n_per_arm", 100)) if ss else 100
        alpha = ss.get("alpha", 0.05) if ss else 0.05
        power = ss.get("power", 0.80) if ss else 0.80
        power_pct = int(power * 100)
        dropout = ss.get("dropout_rate", "10.0%") if ss else "10.0%"
        method_formula = ss.get("method", "Standard Biostatistical Model") if ss else "Standard Biostatistical Model"
        events_req = ss.get("events_required") if ss else None
        formula_latex = ss.get("formula_latex", "") if ss else ""

        ts_params = d.get("cdisc_ts_params", {})
        indication = ts_params.get("INDIC", ts_params.get("PIND", "Target Clinical Indication"))
        obj_prim = ts_params.get("OBJPRIM", f"Evaluate the efficacy and safety of Investigational Product in {indication}")

        md = f"""# CLINICAL TRIAL PROTOCOL

**Protocol Title**: {title}  
**Short Title**: {d.get('badge', phase)} Study in {indication}  
**Therapeutic Area**: {therapeutic_area} | **Category**: {category}  
**Protocol Identification**: PRT-{d.get('id', 'CLIN-001')}-V1.0  
**Phase**: {phase} | **Document Date**: {today}  
**Regulatory Compliance**: ICH GCP E6 (R2), Declaration of Helsinki, 21 CFR Parts 50, 56, 312/812  

---

## 1. PROTOCOL SYNOPSIS

| Key Attribute | Specification |
|:---|:---|
| **Protocol Title** | {title} |
| **Study Phase** | {phase} |
| **Indication** | {indication} |
| **Study Design** | {design_type}, {blinding}, {randomization} |
| **Primary Objective** | {obj_prim} |
| **Hypothesis Framework** | {hypothesis} ({endpoint_type} Primary Endpoint) |
| **Sample Size** | **Total Planned Enrollment: N = {n_total}** ({n_arm1} in Control vs. {n_arm2} in Treatment) |
| **Target Power & Alpha** | {power_pct}% Statistical Power at 2-sided $\\alpha = {alpha}$ (adjusting for {dropout} attrition) |
| **Study Epochs** | {', '.join(epochs)} |
| **Treatment Arms** | {', '.join([a.get('name', 'Arm') for a in arms])} |
| **Duration of Study** | Approximately {len(visits) * 4} weeks active assessment period |

---

## 2. BACKGROUND & SCIENTIFIC RATIONALE

### 2.1 Disease Background & Unmet Need
{indication} represents a significant clinical burden requiring rigorous confirmatory clinical evaluation. Current therapeutic standards leave substantial residual unmet medical needs regarding durability, safety profile, and therapeutic efficacy.

### 2.2 Investigational Product Mechanism of Action
The investigational product targets validated disease-modifying pathways designed to achieve statistically superior clinical endpoints compared to conventional standard of care.

---

## 3. STUDY OBJECTIVES & ICH E9(R1) ESTIMANDS FRAMEWORK

### 3.1 Primary & Secondary Objectives
- **Primary Objective**: {obj_prim}
- **Key Secondary Objective**: Assess safety, tolerability, and durable secondary functional outcomes over {len(visits)*4} weeks.

### 3.2 Primary Estimand Framework (ICH E9(R1))
1. **Target Population**: Adult subjects meeting all eligibility criteria with confirmed diagnosis of {indication}.
2. **Treatment Condition**: Investigational Product vs. Control/Comparator as specified in Section 4.
3. **Variable (Endpoint)**: Change from Baseline in primary clinical endpoint to final protocol assessment.
4. **Intercurrent Events Strategy**:
   - Treatment discontinuation due to AE: *Treatment-Policy Strategy* (all post-discontinuation data collected).
   - Use of rescue medication: *Hypothetical / Composite Strategy*.
5. **Population-level Summary Measure**: Difference in Least Squares (LS) Means / Hazard Ratio / Odds Ratio between active treatment and control.

---

## 4. STUDY DESIGN & ARMS ARCHITECTURE

```
        [Screening Epoch: Inclusion/Exclusion Assessment]
                          │
                          ▼
         [1:1 Randomization & Stratification]
            /                             \\
           ▼                               ▼
 [Arm A: Investigational Regimen]   [Arm B: Control / Comparator]
           │                               │
           ▼                               ▼
   [Treatment Epoch: Protocol Visits & Assessments]
           │                               │
           ▼                               ▼
       [End of Treatment & Safety Follow-up Epoch]
```

### 4.1 Study Arms
"""
        for i, a in enumerate(arms, 1):
            md += f"- **Arm {chr(64+i)} ({a.get('type', 'Arm')})**: {a.get('name', 'Treatment Arm')}\n"

        md += f"""
### 4.2 Study Epochs & Schedule
"""
        for ep in epochs:
            md += f"- **{ep.replace('_', ' ').title()} Epoch**: Protocol procedures, assessments, and safety evaluations.\n"

        md += f"""
---

## 5. SUBJECT POPULATION & ELIGIBILITY CRITERIA

### 5.1 Inclusion Criteria
1. Subject has provided written informed consent prior to any study-related procedure.
2. Age $\\ge 18$ years at the time of signing informed consent.
3. Clinically documented diagnosis of {indication}.
4. ECOG performance status 0–1 (or equivalent clinical functional score).
5. Adequate baseline organ and bone marrow function as assessed by standard laboratory evaluations.
6. Female subjects of childbearing potential and male subjects with partners of childbearing potential agree to use protocol-specified effective contraception.

### 5.2 Exclusion Criteria
1. Prior exposure to investigational agents or therapies targeting the same specific biological pathway within 28 days of Day 1.
2. Active, uncontrolled systemic infection, severe cardiovascular compromise (NYHA Class III/IV), or significant psychiatric comorbidity.
3. History of severe hypersensitivity to any component of the investigational formulation.
4. Pregnant, lactating, or planning pregnancy during the trial period.
5. Concurrent enrollment in any other interventional clinical investigation.

---

## 6. STUDY SCHEDULE OF ACTIVITIES (SoA)

{self._build_soa_table(visits)}

---

## 7. SAFETY MONITORING & ADVERSE EVENT REPORTING

### 7.1 Definitions & Severity Grading
- **Adverse Event (AE)**: Any untoward medical occurrence in a subject administered a pharmaceutical product.
- **Serious Adverse Event (SAE)**: Any untoward medical occurrence that results in death, is life-threatening, requires inpatient hospitalization or prolongation of existing hospitalization, results in persistent or significant disability/incapacity, or is a congenital anomaly/birth defect.
- Severity will be graded in accordance with the NCI Common Terminology Criteria for Adverse Events (CTCAE v5.0) or ISO standards.

### 7.2 Expedited Reporting Timelines
All SAEs, regardless of relationship to investigational product, must be reported to the Sponsor/Safety Committee within **24 hours** of site awareness. Regulatory authorities (FDA, EMA, PMDA) and IRBs/IECs will be notified within statutory expedited timelines (7-day / 15-day reporting).

---

## 8. STATISTICAL CONSIDERATIONS & SAMPLE SIZE JUSTIFICATION

### 8.1 Statistical Hypotheses
The primary analysis tests the {hypothesis.lower()} hypothesis regarding {obj_prim}:
- **Null Hypothesis ($H_0$)**: There is no clinically meaningful difference between the investigational product and control ($H_0: \\theta = 0$ or $H_0: \\text{{HR}} \\ge 1.0$).
- **Alternative Hypothesis ($H_1$)**: The investigational product demonstrates superior clinical efficacy over control ($H_1: \\theta > 0$ or $H_1: \\text{{HR}} < 1.0$).

### 8.2 Sample Size Determination & Statistical Formula
- **Target Statistical Power**: {power_pct}% ($1 - \\beta = {power:.2f}$)
- **Significance Level (Alpha)**: 2-sided $\\alpha = {alpha}$ (1-sided $\\alpha/2 = {alpha/2:.4f}$)
- **Anticipated Dropout / Non-evaluable Rate**: {dropout}
- **Required Evaluated Subjects**: $N_1 = {n_arm1}$ (Control), $N_2 = {n_arm2}$ (Treatment)
- **Total Enrollment Target**: **$N = {n_total}$ subjects**
"""
        if events_req:
            md += f"- **Target Primary Events**: {events_req} events required under Schoenfeld survival model.\n"

        if formula_latex:
            md += f"\n$$\n{formula_latex}\n$$\n\n"

        md += f"""- **Methodology Reference**: {method_formula}

---

## 9. ETHICAL, REGULATORY & DATA MANAGEMENT COMPLIANCE

### 9.1 Good Clinical Practice (GCP)
This study will be conducted in strict conformity with ICH GCP E6 (R2), the ethical principles originating in the Declaration of Helsinki, and applicable regional regulatory statutes.

### 9.2 Institutional Review Board / Independent Ethics Committee
Prior to study initiation, the protocol, informed consent forms, and investigator brochure must be approved by properly constituted IRBs/IECs.

### 9.3 Data Management & 21 CFR Part 11 Electronic Data Capture
All clinical data will be collected using a validated, 21 CFR Part 11 and CDISC-compliant Electronic Data Capture (EDC) system with full audit trail, role-based access control, and electronic signatures. Data will be converted into CDISC SDTM (Study Data Tabulation Model) v1.8 and ADAM (Analysis Data Model) for regulatory submission.
"""
        return md

    def _synthesize_sap_expert(
        self,
        d: Dict[str, Any],
        ss: Optional[Dict[str, Any]] = None
    ) -> str:
        """Constructs an industry-grade ICH E9 Statistical Analysis Plan in Markdown."""
        today = datetime.now().strftime("%B %d, %Y")
        title = d.get("title", "Clinical Trial")
        phase = d.get("phase", "Phase 3")
        therapeutic_area = d.get("therapeutic_area", "General Medicine")
        hypothesis = d.get("hypothesis_type", "superiority").replace("_", " ").title()
        endpoint_type = d.get("endpoint_type", "continuous").title()
        
        arms = d.get("arms", [{"name": "Investigational Product", "type": "Test"}, {"name": "Control / Placebo", "type": "Comparator"}])
        visits = d.get("visits", ["Screening", "Baseline", "Week 4", "Week 12", "End of Study"])
        
        n_total = ss.get("total_enrollment_target", ss.get("total_n", 200)) if ss else 200
        alpha = ss.get("alpha", 0.05) if ss else 0.05
        power = ss.get("power", 0.80) if ss else 0.80
        power_pct = int(power * 100)
        dropout = ss.get("dropout_rate", "10.0%") if ss else "10.0%"
        method_formula = ss.get("method", "Standard Statistical Model") if ss else "Standard Statistical Model"
        formula_latex = ss.get("formula_latex", "") if ss else ""

        ts_params = d.get("cdisc_ts_params", {})
        indication = ts_params.get("INDIC", ts_params.get("PIND", "Target Clinical Indication"))
        obj_prim = ts_params.get("OBJPRIM", f"Primary efficacy endpoint in {indication}")

        md = f"""# STATISTICAL ANALYSIS PLAN (SAP)

**Study Protocol Title**: {title}  
**Protocol Number**: PRT-{d.get('id', 'CLIN-001')} | **SAP Version**: 1.0 (Final)  
**Study Phase**: {phase} | **Therapeutic Area**: {therapeutic_area}  
**Author**: Lead Trial Biostatistician  
**Date of Document**: {today}  
**Regulatory Standards**: ICH E9 (Statistical Principles for Clinical Trials), ICH E9(R1) (Estimands and Sensitivity Analysis)  

---

## 1. INTRODUCTION & OBJECTIVES

This Statistical Analysis Plan (SAP) outlines the comprehensive statistical methodology, definition of analysis populations, hypothesis testing procedures, model specifications, missing data handling algorithms, and Table, Figure, and Listing (TFL) shells for Protocol **PRT-{d.get('id', 'CLIN-001')}**.

### 1.1 Study Objectives
- **Primary Objective**: Demonstrate the statistical {hypothesis.lower()} of the investigational therapy relative to control with respect to {obj_prim}.
- **Secondary Objectives**: Characterize safety/tolerability profiles, adverse events, laboratory shifts, and secondary clinical response parameters.

---

## 2. STUDY DESIGN & RANDOMIZATION ARCHITECTURE

### 2.1 Design Structure
This is a prospective, {d.get('design_type', 'Parallel Group').lower()}, {d.get('blinding', 'Double-Blind').lower()} study. Subjects are randomized in a **{d.get('randomization', '1:1')}** allocation ratio stratified by baseline clinical prognostic factors.

### 2.2 Treatment Arm Coding
"""
        for i, a in enumerate(arms, 1):
            md += f"- **Treatment Arm {i} ({a.get('type', 'Arm')})**: `{a.get('code', f'ARM_{i}')}` — {a.get('name', 'Arm')}\n"

        md += f"""
---

## 3. ANALYSIS POPULATIONS & SETS

In accordance with ICH E9, the following analysis populations are pre-specified:

| Analysis Set | Acronym | Inclusion Criteria | Primary Usage |
|:---|:---:|:---|:---|
| **Intention-to-Treat / Full Analysis Set** | **ITT / FAS** | All randomized subjects who signed informed consent, analyzed according to assigned randomized arm regardless of treatment received. | **Primary Efficacy Analyses** |
| **Modified Intention-to-Treat** | **mITT** | All randomized subjects who received $\\ge 1$ dose of study treatment and have $\\ge 1$ post-baseline efficacy assessment. | **Secondary Efficacy / Sensitivity** |
| **Per-Protocol Set** | **PPS** | Subset of ITT subjects who completed the treatment epoch without major protocol deviations (e.g. eligibility violations, non-compliance $< 80\\%$). | **Robustness / Supplementary Efficacy** |
| **Safety Analysis Set** | **SAF** | All subjects who received $\\ge 1$ dose of study treatment, analyzed according to actual treatment received. | **All Safety & Tolerability Analyses** |

---

## 4. GENERAL STATISTICAL PRINCIPLES

### 4.1 Significance Level & Confidence Intervals
- All primary and secondary statistical hypothesis tests will be conducted at a two-sided significance level of **$\\alpha = {alpha}$** (or one-sided $\\alpha = {alpha/2:.4f}$ where appropriate).
- Two-sided **95% Confidence Intervals (CIs)** will be calculated for all effect estimates, odds ratios, hazard ratios, and between-group treatment differences.

### 4.2 Multiplicity Adjustments
To preserve the family-wise type I error rate (FWER) at $\\alpha = {alpha}$, a pre-specified hierarchical gatekeeping testing strategy or graphical Hochberg procedure will be implemented across primary and key secondary endpoints.

### 4.3 Missing Data & Sensitivity Strategies (ICH E9 R1)
- **Primary Method**: Mixed-Model Repeated Measures (**MMRM**) utilizing an unstructured covariance matrix under the Missing-At-Random (MAR) assumption.
- **Sensitivity Imputation**: Pattern-Mixture Models with Jump-to-Reference / Tipping-Point sensitivity analyses exploring non-random departure (MNAR).

---

## 5. PRIMARY EFFICACY ENDPOINT ANALYSIS

### 5.1 Endpoint Definition & Operationalization
- **Primary Endpoint**: {obj_prim} assessed from Baseline to primary landmark visit ({visits[-1] if visits else 'Final Visit'}).

### 5.2 Statistical Model Specification
"""
        if "survival" in d.get("endpoint_type", "").lower() or "oncology" in d.get("category", "").lower():
            md += """- **Primary Model**: Stratified Log-Rank Test and Cox Proportional Hazards Model adjusting for baseline stratification factors.
- **Effect Estimate**: Hazard Ratio (HR) with 95% Wald Confidence Interval and associated p-value.
- **Kaplan-Meier Methodology**: Product-limit survivor function estimates plotted with 95% Hall-Wellner confidence bands. Median survival times estimated with Brookmeyer-Crowley method.
"""
        elif "binary" in d.get("endpoint_type", "").lower() or "device" in d.get("category", "").lower():
            md += f"""- **Primary Model**: Farrington-Manning Score Test / Stratified Cochran-Mantel-Haenszel (CMH) test.
- **Effect Estimate**: Difference in response proportions ($\\Delta = p_T - p_C$) with 95% Newcombe Wilson score confidence intervals.
- **Non-Inferiority / Superiority Decision Rule**: Reject null hypothesis if the lower bound of the 95% CI exceeds the pre-specified margin $\\delta = -{d.get('default_params', {}).get('delta_margin', 0.08)}$.
"""
        else:
            md += """- **Primary Model**: Analysis of Covariance (**ANCOVA**) or Mixed-Model Repeated Measures (**MMRM**).
- **Model Formula**: `Change = Intercept + Treatment + Baseline_Value + Stratification_Factors + Visit + Treatment*Visit + Error`
- **Effect Estimate**: Least Squares (LS) Mean difference between treatment arms with standard errors, 95% CIs, and t-test p-value.
"""

        md += f"""
### 5.3 Sample Size & Statistical Power
- Total enrolled sample size: **$N = {n_total}$**
- Target Power: **{power_pct}%** at $\\alpha = {alpha}$
- Statistical derivation methodology: `{method_formula}`
"""
        if formula_latex:
            md += f"\n$$\n{formula_latex}\n$$\n\n"

        md += """
---

## 6. SECONDARY & EXPLORATORY ENDPOINTS

### 6.1 Key Secondary Efficacy Analyses
- Continuous secondary endpoints evaluated via MMRM / ANCOVA.
- Binary responder rates evaluated via logistic regression and Fisher's exact tests.
- Longitudinal trajectories summarized with mean $\\pm$ SE trajectory plots over study visits: {', '.join(visits)}.

### 6.2 Subgroup Analyses
Pre-specified subgroup consistency will be evaluated across:
- Age categories ($<65$ vs. $\\ge 65$ years)
- Sex (Male vs. Female)
- Baseline disease severity / biomarker subgroups
- Forest plots displaying point estimates and 95% CIs will be constructed for each subgroup factor.

---

## 7. SAFETY & TOLERABILITY ANALYSES

All safety analyses will be conducted on the **Safety Analysis Set (SAF)**.

### 7.1 Adverse Events (AEs)
- All AEs will be mapped and coded using the current **MedDRA** version dictionary by System Organ Class (SOC) and Preferred Term (PT).
- **Treatment-Emergent Adverse Events (TEAEs)** defined as any AE with onset on or after the first dose of study treatment up to 30 days post-last dose.
- Summary tables will display:
  - Overall incidence of TEAEs, SAEs, Deaths, and TEAEs leading to discontinuation.
  - TEAE incidence by SOC and PT in descending order of frequency.
  - Severity-graded TEAEs (CTCAE Grades 1–5).
  - Treatment-related TEAEs (definite, probable, possible).

### 7.2 Clinical Laboratory Evaluations
- Summary statistics (mean, SD, median, Q1, Q3, min, max) of observed values and changes from baseline across all laboratory panels (Hematology, Serum Chemistry, Urinalysis).
- **Shift Tables**: Baseline to worst post-baseline CTCAE toxicity grade / reference range classification (Low, Normal, High).

### 7.3 Vital Signs & ECG
- Summary of vital signs (Systolic/Diastolic Blood Pressure, Pulse, Temperature, Respiratory Rate) by visit.
- Treatment-emergent clinically notable vital sign abnormalities.
- 12-lead ECG intervals (PR, QRS, QT, QTcB, QTcF) and categorical QTcF prolongation ($>450$ ms, $>480$ ms, $>500$ ms, increase $>30$ ms, $>60$ ms).

---

## 8. INTERIM ANALYSES & DATA MONITORING COMMITTEE (DMC)

- An independent **Data Monitoring Committee (DMC / DSMB)** will monitor unblinded safety and efficacy data.
- Efficacy stopping boundaries governed by Lan-DeMets alpha-spending approximation to **O'Brien-Fleming** boundaries.
- Non-binding futility stopping rules evaluated at scheduled interim looks.

---

## 9. TABLE, FIGURE, AND LISTING (TFL) SHELLS MASTER INDEX

### 9.1 Summary Tables (14.x)
- **Table 14.1.1**: Subject Disposition and Analysis Population Allocation (All Screened Subjects)
- **Table 14.1.2**: Demographic and Baseline Clinical Characteristics (ITT & Safety Populations)
- **Table 14.1.3**: Prior and Concomitant Medications (Safety Population)
- **Table 14.2.1**: Primary Efficacy Analysis — Model Estimates and Treatment Differences (ITT Population)
- **Table 14.2.2**: Sensitivity Analysis for Primary Efficacy Endpoint (mITT & PP Populations)
- **Table 14.2.3**: Key Secondary Efficacy Endpoints by Visit (ITT Population)
- **Table 14.3.1**: Overall Summary of Treatment-Emergent Adverse Events (Safety Population)
- **Table 14.3.2**: TEAEs by MedDRA System Organ Class and Preferred Term (Safety Population)
- **Table 14.3.3**: Serious Adverse Events and Deaths (Safety Population)
- **Table 14.3.4**: Clinical Laboratory Shift Tables from Baseline to Worst Post-Baseline Grade (Safety Population)

### 9.2 Figures (14.x)
- **Figure 14.2.1**: Primary Efficacy Endpoint Over Time by Treatment Arm (Mean $\\pm$ SE)
- **Figure 14.2.2**: Kaplan-Meier Curves for Time-to-Event Efficacy Outcomes with Number at Risk
- **Figure 14.2.3**: Forest Plot of Treatment Effect Across Key Subgroups

### 9.3 Subject Data Listings (16.x)
- **Listing 16.2.1**: Subject Discontinuations and Protocol Deviations
- **Listing 16.2.4**: Demographics and Baseline Characteristics
- **Listing 16.2.7**: Serious Adverse Events and Deaths
- **Listing 16.2.8**: Individual Laboratory Test Abnormalities
"""
        return md

    def _generate_protocol_llm(
        self,
        design: Dict[str, Any],
        sample_size: Optional[Dict[str, Any]] = None,
        custom_prompt: Optional[str] = None
    ) -> Optional[str]:
        """Calls external LLM (Gemini, Claude, OpenAI) to generate tailored clinical protocol."""
        prompt = f"""You are an elite Clinical Trial Design Biostatistician and Regulatory Medical Writer.
Write a comprehensive, regulatory-grade ICH GCP E6 (R2) Clinical Trial Protocol for the following study design:

Study Design Archetype:
{json.dumps(design, indent=2)}

Biostatistical Sample Size Calculation:
{json.dumps(sample_size or {}, indent=2)}

Custom Instructions:
{custom_prompt or 'Generate complete ICH E6 compliant clinical protocol with full scientific rationale, estimands, eligibility, schedule of activities, safety reporting, and statistical power justification.'}

Format the output in clear, professional Markdown with headers (#, ##, ###), markdown tables for synopsis & schedule of activities, and LaTeX math formulas ($...$) where applicable.
"""
        return self._invoke_llm(prompt)

    def _generate_sap_llm(
        self,
        design: Dict[str, Any],
        sample_size: Optional[Dict[str, Any]] = None,
        custom_prompt: Optional[str] = None
    ) -> Optional[str]:
        """Calls external LLM (Gemini, Claude, OpenAI) to generate tailored Statistical Analysis Plan."""
        prompt = f"""You are a Principal Biostatistician at a global clinical research organization.
Write a comprehensive, regulatory-grade ICH E9 and ICH E9(R1) Statistical Analysis Plan (SAP) for the following study design:

Study Design Archetype:
{json.dumps(design, indent=2)}

Biostatistical Sample Size Calculation:
{json.dumps(sample_size or {}, indent=2)}

Custom Instructions:
{custom_prompt or 'Generate complete ICH E9 compliant SAP with analysis sets (ITT, mITT, PP, Safety), statistical modeling, missing data estimands (MMRM, tipping-point), safety tables, and TFL master index.'}

Format the output in clear, professional Markdown with headers (#, ##, ###), markdown tables, and LaTeX math formulas ($...$) where applicable.
"""
        return self._invoke_llm(prompt)

    def _invoke_llm(self, prompt: str) -> Optional[str]:
        """Invokes the configured LLM provider."""
        if self.provider == "gemini":
            try:
                from google import genai
                client = genai.Client(api_key=self.api_key)
                model = self.model_name or "gemini-2.5-pro"
                response = client.models.generate_content(
                    model=model,
                    contents=prompt
                )
                return response.text
            except Exception as e:
                logger.warning(f"Google Gemini invocation error: {e}")
                return None

        elif self.provider == "anthropic":
            try:
                import anthropic
                client = anthropic.Anthropic(api_key=self.api_key)
                model = self.model_name or "claude-3-7-sonnet-20250219"
                msg = client.messages.create(
                    model=model,
                    max_tokens=4096,
                    messages=[{"role": "user", "content": prompt}]
                )
                return msg.content[0].text
            except Exception as e:
                logger.warning(f"Anthropic Claude invocation error: {e}")
                return None

        elif self.provider == "openai":
            try:
                import openai
                client = openai.OpenAI(api_key=self.api_key)
                model = self.model_name or "gpt-4o"
                resp = client.chat.completions.create(
                    model=model,
                    messages=[{"role": "user", "content": prompt}]
                )
                return resp.choices[0].message.content
            except Exception as e:
                logger.warning(f"OpenAI invocation error: {e}")
                return None

        return None
