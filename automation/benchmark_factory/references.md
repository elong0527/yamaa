# Benchmark Factory Reference Library

Curated external sources for grounding yamaa benchmark work items
(create / enhance / combine / retire / recalibrate). A work item should
cite at least one entry from this file when its spec claims real-world
relevance or borrows edge cases from published material.

Conventions used below:

- TA = therapeutic area. "Cross-TA" means the source applies to many areas.
- Work-item grounding = how to use the source in a work item or judge check.
- Some PharmaSUG entries link to official program listings (search the title
  on the page); exact lexjansen PDF paths could not be verified, so program
  pages are linked instead of guessed URLs.

## PharmaSUG proceedings

- **ADaM Design for Prostate Cancer Efficacy Endpoints Based on PCWG3**
  (Deng, Fan, Li, 2024) -
  https://pharmasug.org/conferences/pharmasug-2024-us/pharmasug-2024-paper-presentations/ -
  Program listing; search the title. PCWG3 oncology endpoints (PSA response,
  time to PSA progression, rPFS/ORR), ADaM design, traceability.
  TA: oncology (prostate). Grounding: edge cases for ADTTE censoring and
  event-date logic for composite oncology endpoints.

- **Programming Considerations in Deriving PFS2 (PFS on Next-Line Therapy)**
  (McConnell, Peng, 2024) -
  https://pharmasug.org/conferences/pharmasug-2024-us/pharmasug-2024-paper-presentations/ -
  Program listing; search the title. Deriving PFS2 in ADTTE; second-event
  time-to-event programming. TA: oncology. Grounding: event-date stacking,
  censoring, and SRCDOM/SRCVAR traceability for secondary TTE parameters.

- **SAS Macro for Derivation of Best Overall Response per RECIST 1.1**
  (Zhong et al., 2017) -
  https://www.lexjansen.com/pharmasug/2017/AD/PharmaSUG-2017-AD24.pdf -
  RECIST 1.1 best overall response from time-point overall responses
  (CR/PR/SD/PD/NE); PR-SD-PR sequences, unconfirmed CR/PR, tabulated
  decision rules. TA: oncology. Grounding: the confirmation-timing tables
  are ready-made judge test vectors for an ADRS-BOR benchmark.

- **The Phantom of the ADaM: adding missing records to BDS datasets**
  (Drach, 2023) -
  https://pharmasug.org/conferences/pharmasug-2023-us/paper-presentations/ -
  Program listing; search the title. "Phantom" derived records with missing
  AVAL/AVALC for missed visits; scenario ladder from simple to complex.
  TA: cross-TA. Grounding: enhances a BDS benchmark with phantom-record
  generation, with natural difficulty tiers.

- **Exploit the Window of Opportunity: Analysis Windowing Variables**
  (Watson, Dennis, Miller, 2024) -
  https://pharmasug.org/conferences/pharmasug-2024-us/pharmasug-2024-paper-presentations/ -
  Program listing; search the title. SAP-driven analysis windows;
  AWTARGET/AWTDIFF/AWLO/AWHI and per-window record-selection rules.
  TA: cross-TA. Grounding: tie-breaking and window-selection rules as golden
  logic for BDS windowing derivations.

- **How to build up the Laboratory data flow that fits for analysis purpose**
  (Liu, 2023) -
  https://pharmasug.com.cn/resources/2023/2023-09/Pharmasug-China-2023-PO151.pdf -
  Full SDTM LB to ADLB flow: unit standardization, LLOQ/ULOQ imputation,
  CTCAE grading (BTOXGR to ATOXGR), worst-grade with ANL01FL.
  TA: cross-TA (lab). Grounding: direct source for ADLB shift/toxicity
  enhancements; the worst-grade chain is judge-verifiable.

- **Create a Shift Summary of Laboratory Values in CTCAE Grade to Worst
  Grade Abnormal Value using R and SASSY** (Yuan, 2024) -
  https://pharmasug.org/conferences/pharmasug-2024-us/pharmasug-2024-paper-presentations/ -
  Program listing; search the title. CTCAE grade shift to worst abnormal
  value. TA: cross-TA (lab shift). Grounding: independent second source to
  cross-check worst-grade/shift golden values.

- **Improving CDISC BDS Requirements on PARAMCD for ADaM Automation**
  (Bo, Chen, Zhao, 2023) -
  https://pharmasug.com.cn/resources/2023/2023-08/Pharmasug-China-2023-DS159.pdf -
  Metadata-only ADaM automation via a DSL/interpreter; PARAMCD single-key
  vs PARQUAL composite keys (oncology ADRESP example); DTYPE.
  TA: cross-TA (methodology; oncology example). Grounding: derivation-spec
  precision; PARAMCD-scoped derivations in task specs.

- **Leverage and Enhance CDISC TAUGs for Traceability of Efficacy ADaM in
  Oncology** (Cui, 2023) -
  https://pharmasug.org/conferences/pharmasug-2023-us/paper-presentations/ -
  Program listing; search the title. Intermediate ADEVENT/ADRESP/ADDATES
  feeding ADTTE; SRCDOM/SRCVAR/SRCSEQ traceability; RECIST confirmation
  complexity. TA: oncology. Grounding: traceability source for SRCDOM/
  SRCVAR/SRCSEQ chains and multi-stage SDTM to ADaM pipeline work items.

- **Handling Anti-Drug Antibody (ADA) Data for Efficient Analysis**
  (Sundaram, Maruthavanan, 2023) -
  https://pharmasug.org/conferences/pharmasug-2023-us/paper-presentations/ -
  Program listing; search the title. ADA screening/confirmation/NAb/titer
  mapping into SDTM IS; ADaM-level ADA variables; baseline-positive vs
  post-baseline titer handling. TA: biologics/immunogenicity. Grounding:
  IS to ADaM work items with tricky baseline-positive ADA edge cases.

- **Automated Harmonization: Unifying ADaM Generation and Define.xml
  through ADaM Specs** (Shao, Zou, 2024) -
  https://pharmasug.org/conferences/pharmasug-2024-us/pharmasug-2024-paper-presentations/ -
  Program listing; search the title. Unifying ADaM generation and define.xml
  from a single spec (metadata-driven). TA: cross-TA. Grounding:
  derivation-metadata to dataset consistency judge checks.

- **Making a List, Checking it Twice (Part 1): Specifying and Validating
  Analysis Datasets** (Li, Collins, 2011) -
  https://www.pharmastat.com/wp-content/uploads/2011/06/PharmaStat-Tables-Figures-Production-PharmaSUG-2011.pdf -
  Machine-readable ADaM specs driving generation; independent
  double-programming validation; define.xml generation; ADSL
  SAFFL/ITTFL/SEXN examples. TA: cross-TA. Grounding: the factory's
  validation philosophy (independent-programming compare as judge pattern)
  plus ADSL flag derivations.

- **Progression-Free Survival (PFS) Analysis in Solid Tumor Clinical Studies**
  (Li, 2019) -
  https://pharmasug.org/proceedings/2019/SS/PharmaSUG-2019-SS-027.pdf -
  PFS event/censoring rules with worked ADTTE examples: no post-baseline
  tumor assessment -> censored at the randomization date (PFS = 1 day);
  non-study anti-cancer intervention before a PD -> censored at the last
  adequate assessment before the intervention, the later PD not an event.
  TA: oncology. Grounding: PFS censoring conventions for ADTTE
  (new-therapy cut, no-assessment fallback) and judge test vectors.

- **Evaluation of the Process to Create CLINSITE define.xml: Macro Approach
  vs. ADCLIN Spec** (Zhong, Jiang, Teng, 2025) -
  https://pharmasug.org/proceedings/2025/SS/PharmaSUG-2025-SS-135.pdf -
  FDA BIMO CLINSITE site-level dataset ("Clinical Site Data Elements"):
  ARM with origin Assigned, SAFPOP derived from the number of subjects
  treated at the site, SITEID as predecessor from ADSL.SITENUM; macro vs
  ADaM-spec (ADCLIN) approaches to define.xml generation. TA: cross-TA
  (regulatory submission). Grounding: variable origins and site-level
  aggregation rules for a BIMO CLINSITE benchmark; the define.xml process
  debate is metadata, out of scope for dataset benchmarks.

## CDISC standards

- **ADaM Basic Data Structure for Time-to-Event Analyses v1.0** (CDISC, 2012) -
  https://www.cdisc.org/standards/foundational/adam -
  Time-to-Event example (p. 19): PFS records with CNSR/EVNTDESC/CNSDTDSC,
  including "NEW ANTI-CANCER THERAPY. CENSORED AT TIME OF LAST ASSESSMENT."
  and "NO BASELINE ASSESSMENT. CENSORED AT TIME OF RANDOMIZATION."
  TA: cross-TA. Grounding: canonical PFS censoring conventions and
  censoring-reason descriptions for ADTTE benchmarks.

- **CDISC Controlled Terminology** (CDISC and NCI EVS, quarterly) -
  https://evs.nci.nih.gov/ftp1/CDISC/SDTM/SDTM%20Terminology.txt and
  https://evs.nci.nih.gov/ftp1/CDISC/ADaM/ADaM%20Terminology.txt -
  SDTM ECG test codes QTAG (C117783, "QT Interval, Aggregate"), RRAG
  (C117791, "RR Interval, Aggregate") and QTCFAG (C117786, "QTcF Interval,
  Aggregate"), each aggregated over the beats of one ECG; ADaM derivation
  type AVERAGE (C81209, "average value over a defined set of records").
  TA: cross-TA. Grounding: exact codes and labels for ADEG and other
  BDS work items. SDTM PK Parameters Code (C85839) CMAX, AUCLST, AUCIFO and
  TMAX, with the PK Parameters (C85493) submission values "Max Conc", "AUC to
  Last Nonzero Conc", "AUC Infinity Obs" and "Time of CMAX Observation" ("Time
  of CMAX" is a synonym); Not Done (C66789) "NOT DONE". Grounding (ADPP):
  PARAMCD and PARAM literals and the not-done pre-dose sample.
  For adam-adpp-accumulation-ratio: ARAUC "Accumulation Ratio AUCTAU" (AUC
  over the dosing interval at steady state divided by that over the initial
  dosing interval), ARCMAX "Accumulation Ratio Cmax", AUCTAU and CMAX; PKUNIT
  (C85494) RATIO, h*ng/mL, ng/mL: the PARAMCD, PARAM and AVALU literals and
  the direction of a PK ratio.
  For adam-adpp-dose-normalized: the dose-normalized CMAXD "Max Conc Norm by
  Dose", AUCLSTD "AUC to Last Nonzero Conc Norm by Dose" and AUCIFOD "AUC
  Infinity Obs Norm by Dose", each "divided by the dose" (no Tmax or half-life
  code is dose-normalized), and PKUDMG (ng/mL/mg, h*ng/mL/mg).

- **SDTMIG v3.3** (CDISC, 2018) -
  https://www.cdisc.org/standards/foundational/sdtmig/sdtmig-v3-3/html -
  Section 6.3.3 ECG Test Results (EG): one record per ECG observation per
  replicate per time point; "EGREPNUM is used to differentiate between
  multiple repetitions of a test within a given time frame"; QTCFAG is
  used for Fridericia's formula; the extracted-replicate example gives
  the results of one replicate the same EGREPNUM. TA: cross-TA (QT).
  Grounding: replicate identity for ECG averaging work items. Section 6.3.11.1
  Pharmacokinetics Concentrations (PC): PCSTRESN "should store all numeric test
  results or findings"; the worked example has a parent and a metabolite record
  from one sample and time point, and writes a result below the limit of
  quantification as "<0.1" in PCSTRESC with no PCSTRESN: one record per analyte
  and no numeric value for a below-limit result
  (adam-adeg-time-matched-concentration). PC and PP domains: PP is "One record
  per PK parameter per time-concentration profile per modeling method per
  subject"; "The values in PPRFTDTC should be the same as that in PCRFTDTC for
  related records". The PC example holds the pre-dose sample at PCTPT "PREDOSE"
  and a below-limit result as PCSTRESC "<0.1" with no PCSTRESN and PCLLOQ 0.10.
  Grounding (ADPP): the PP-to-PC link and the pre-dose sample of
  adam-adpp-carryover-exclusion.
  For adam-adpp-dose-normalized: PP assumption 3 (PKUDMG/PKUDUG for parameters
  normalized by a dose in mg or ug) and EX (6.1.3.1): EXDOSE is the amount per
  administration, EXDOSFRQ the number of administrations per period, natural
  key STUDYID, USUBJID, EXTRT, EXSTDTC; the dose of a profile is the EX record
  starting at its PPRFTDTC.
  For adam-adpp-accumulation-ratio: 6.3.11.2 PPSTAT "Used to indicate that a
  parameter was not calculated", with PPREASND; the PP examples show Day 1
  and a later day without calling either steady state (the not-calculated
  case and the steady-state choice a work item must state).

- **What is ADNCA and Why is it Important?** (Pinnacle 21 webinar slides,
  2024) - https://help.pinnacle21.certara.net/webinars/What-is-ADNCA.pdf -
  Public summary of the ADaMIG for Non-compartmental Analysis Input Data
  v1.0 (whose text needs a CDISC sign-in): AVALU "Analysis Value Unit",
  PKUNIT codelist, Required. TA: cross-TA (clinical pharmacology).
  Grounding: AVALU on ADPC/ADPP records.

## pharmaverse

- **admiral** - https://github.com/pharmaverse/admiral -
  Cross-TA core derivations: AVAL/CHG/PCHG (derive_var_base/chg/pchg),
  computed parameters, TTE (derive_param_tte), CTCAE grading, dose
  intensity, LOCF, date imputation, period datasets. Grounding: yamaa
  CHG/PCHG/shift benchmarks specified against admiral's reference edge
  cases; single-parameter CHG/PCHG with ABLFL rules; one-endpoint TTE with
  custom event/censor sources.

- **pharmaversesdtm** - https://github.com/pharmaverse/pharmaversesdtm/ -
  Test SDTM corpus (rs_onco, rs_onco_irecist, oe_ophtha, ex_ophtha, ...).
  TA: cross-TA plus TA variants. Grounding: canonical versioned input
  fixtures for SDTM to ADaM tasks (e.g., RECIST rows from rs_onco).

- **pharmaverseadam** - https://github.com/pharmaverse/pharmaverseadam/ -
  Test ADaM data (adsl, adrs_onco, adtte_onco, adbcva_ophtha, adce_vaccine,
  adis_vaccine, ...). TA: cross-TA. Grounding: off-the-shelf expected-output
  oracles, e.g. "reproduce adrs_onco BOR rows from rs_onco". Caveat: test
  data is un-QC'd; verify per dataset before using as golden truth.

- **admiralonco** - https://github.com/pharmaverse/admiralonco -
  Oncology: derive_param_bor() (NE propagation), confirmed response/BOR,
  clinical benefit; pre-defined event/censor sources for OS/PFS/DOR/TTR;
  RECIST 1.1 / iRECIST / IMWG / GCIG / PCWG3. Grounding: RECIST 1.1
  confirmed response + BOR is the canonical medium-complexity benchmark;
  PFS via derive_param_tte() with PD/death events and censor sources;
  goldens checkable against adrs_onco.

- **admiralophtha** - https://github.com/pharmaverse/admiralophtha -
  Ophthalmology: derive_var_bcvacritxfl() (CRITx/CRITxFL for letter
  gain/loss thresholds), LogMAR to ETDRS conversion, study-eye derivation.
  Grounding: bcvacritxfl is an ideal small benchmark - one well-documented
  function with exact criterion semantics; LogMAR to ETDRS is a pure-function
  golden task. Note: no ophthalmology TAUG exists.

- **admiralvaccine** - https://github.com/pharmaverse/admiralvaccine -
  Vaccines: derive_diam_to_sev_records() (redness/swelling diameter to
  severity), fever/severity scales from diary FACE data (ADCE/ADFACE/ADIS).
  Grounding: diameter to severity threshold mapping - compact, clear
  expected values.

- **admiralpeds** - https://github.com/pharmaverse/admiralpeds -
  Pediatrics: derive_params_growth_age() - WHO/CDC LMS z-scores/percentiles
  for weight/height/BMI/head circumference. Grounding: self-contained numeric
  benchmark with the authoritative formula Z=((obs/M)^L-1)/(L*S); BMI
  percentiles and WHO correction rules supply edge cases.

- **admiralmetabolic** - https://pharmaverse.github.io/admiralmetabolic/ -
  Metabolic (obesity, T2D, NAFLD): HOMA-IR, FLI (Fatty Liver Index),
  waist-to-height/waist-to-hip ratios. Grounding: FLI is a realistic
  multi-input composite-index benchmark - requires transposing ADVS values
  into ADLB first, testing cross-dataset assembly plus formula accuracy.

- **admiralneuro** - https://github.com/pharmaverse/admiralneuro -
  Neuroscience (Alzheimer's): compute_centiloid() (amyloid PET SUVR to
  Centiloid by tracer/pipeline/reference region). Grounding: compact
  tracer-dependent conversion benchmark with exact expected values.

- **admiral ECG functions and ADEG template** (admiral 1.5.0 and main) -
  https://pharmaverse.github.io/admiral/cran-release/reference/compute_qtc.html,
  https://pharmaverse.github.io/admiral/cran-release/reference/derive_param_qtc.html,
  https://github.com/pharmaverse/admiral/blob/main/inst/templates/ad_adeg.R -
  `compute_qtc()` (Fridericia: QT over the cube root of RR / 1000, QT and
  RR in msec); the template derives QTcF per collected record, then adds
  `DTYPE = "AVERAGE"` records per PARAMCD with `derive_summary_records()`
  "if least 2 records available". TA: QT. Grounding: order of correction
  and averaging, and the minimum-count choice a work item must state.
  derive_basetype_records()
  (https://pharmaverse.github.io/admiral/cran-release/reference/derive_basetype_records.html)
  sets BASETYPE when a parameter has more than one baseline definition
  (ADaMIG v1.3 section 4.2.1.6): non-null wherever BASE is, one value per
  definition; the template flags ABLFL and carries BASE by subject,
  parameter and BASETYPE (derive_var_base stops on two baseline records in
  a group) and leaves CHG on baseline records "to producer choice".
  Grounding: several baselines per parameter keyed by BASETYPE
  (adam-adeg-time-matched-baseline).

- **admiral analysis flags and the ADPP template** (admiral 1.5.0) -
  https://pharmaverse.github.io/admiral/cran-release/articles/pk_adnca.html,
  https://pharmaverse.github.io/admiral/cran-release/articles/bds_finding.html,
  https://pharmaverse.github.io/admiral/cran-release/reference/derive_var_extreme_flag.html,
  https://github.com/pharmaverse/admiral/blob/v1.5.0/inst/templates/ad_adpp.R -
  pk_adnca: "This is also an opportunity to derive analysis flags" (ANL01FL,
  ANL02FL, set to "Y"); bds_finding: "an analysis flag (e.g. ANLxxFL) may be
  used to choose the appropriate record for analysis", and TRTA/TRTP match ADSL
  treatment variables such as TRTSEQP; derive_var_extreme_flag() defaults
  true_value "Y", false_value NA_character_; ad_adpp.R gives each PPTESTCD its
  PARAMCD and PARAM and sets AVAL = PPSTRESN. TA: cross-TA (clinical
  pharmacology). Grounding: Y-or-null analysis flags and the PP-to-ADPP mapping
  of the ADPP work items (public stand-in for the ADaMIG flag text, which needs
  a CDISC sign-in).
  For adam-adpp-dose-normalized: the template's parameter lookup names CMAXD
  "Max Conc Norm by Dose" and AUCIFOD "AUC Infinity Obs Norm by Dose" with no
  unit in PARAM; no dose-normalized record is added without a dose.
  For adam-adpp-accumulation-ratio: derive_param_computed()
  (https://pharmaverse.github.io/admiral/cran-release/reference/derive_param_computed.html)
  adds a record only when each contributing parameter has exactly one record
  and no contributing value is missing (keep_nas default): which computed
  records exist when an input value is missing.

- **PKNCA** (humanpred; not a pharmaverse package) -
  https://humanpred.github.io/pknca/ -
  Open-source R package for non-compartmental analysis (site 0.12.1.9002,
  release 0.12.1). Its "Post-Processing" article: the built-in exclusion
  functions "will exclude all results that either apply to the current value or
  are dependents of the current value (parameters that depend on it)";
  exclude() records a reason for exclusion per result. TA: cross-TA (clinical
  pharmacology). Grounding: an exclusion acts on a result and its dependents,
  not on the subject's other parameters (adam-adpp-comparison-sets judges pairs
  per parameter).

- **aNCA** (pharmaverse, commit f8d921b) -
  https://github.com/pharmaverse/aNCA -
  Shiny/R NCA application. Its CDISC export (R/export_cdisc.R) builds ADPP
  with PARAMCD = PPTESTCD, PARAM = PPTEST, AVAL = PPSTRESN and AVALU =
  PPSTRESU; its manual uses the actual dose DOSEA "for dose normalization
  of parameters like Cmax and AUC". TA: PK. Grounding: ADPP variable
  conventions and which parameters adam-adpp-dose-normalized normalizes.
  Its "Ratio Calculations" article
  (https://pharmaverse.github.io/aNCA/articles/ratio-calculations.html)
  defines the accumulation ratio AR{param} as the parameter at a later dose
  profile over the first-dose profile (ATPTREF), within subject when "Mean
  across subjects" is no; the later profile is the study's choice
  (adam-adpp-accumulation-ratio).

- **PKNCA pk.calc.dn** (PKNCA 0.12.1; not a pharmaverse package) -
  https://humanpred.github.io/pknca/reference/pk.calc.dn.html -
  "Determine dose normalized NCA parameter": the parameter divided by the
  dose (pk.calc.dn(90, 10) is 9), registered for AUC, AUMC and
  concentration parameters (cmax, auclast, aucinf.obs, ...) but not Tmax or
  half-life; an interval without a dose gets a missing dose. TA: PK.
  Grounding: the dose-normalization formula and its parameter scope in
  adam-adpp-dose-normalized.

Pin package versions in work-item specs. Mid-2026 reference points:
admiral 1.5.0, admiralonco 1.4.1, admiralophtha 1.5.0, admiralvaccine 0.6.0,
admiralpeds 0.4.0, admiralmetabolic 0.3.0, admiralneuro 0.3.0,
pharmaversesdtm 1.5.0; PKNCA 0.12.1 (not pharmaverse), aNCA at commit f8d921b.

## CDISC therapeutic area user guides (TAUGs)

TA library (authoritative listing):
https://www.cdisc.org/standards/therapeutic-areas

TAUGs are SDTM/CDASH-oriented with sparse ADaM coverage; use them for
TA-specific modeling rules, not derivation logic. Gaps worth knowing:
no ophthalmology TAUG, no hematologic-malignancy TAUG (oncology TAUGs are
solid-tumor only).

- Vaccines v1.1 -
  https://www.cdisc.org/standards/therapeutic-areas/vaccines/vaccines-therapeutic-area-user-guide-v1-1 -
  Grounding: ADLB-style seroresponse (4-fold rise, seroprotection) from IS
  titers with LLOQ handling.

- Colorectal Cancer v1.0 and Prostate Cancer v1.0 (see TA library) -
  Grounding: derive ADRS best overall response plus ADTTE PFS from
  lesion-level TR through RS to subject-level response, with new-lesion
  and progression censoring rules.

- Alzheimer's v2.0.1 (see TA library) -
  Grounding: ADQS change-from-baseline plus time-to-MCI-conversion ADTTE.

- HIV v1.0 (see TA library) -
  Grounding: virologic suppression at Week 48 with the FDA snapshot
  algorithm.

- Huntington's Disease v1.0 (see TA library) -
  Grounding: UHDRS total motor score and composite cUHDRS from QS items.

- PTSD v1.0 (see TA library) -
  Grounding: CAPS-5 total severity plus a 30% responder flag.

- Nutrition v1.0 and T1D Screening/Staging/Monitoring v1.0 (see TA library) -
  Grounding: T1D Stage 1/2/3 classification from autoantibody count plus
  dysglycemia; other disease-classification flags.

- DMD v1.0 (see TA library) -
  Grounding: annualized 6MWD change.

Other seeds: RA -> ACR20/50/70 flags from joint counts, globals, HAQ, CRP;
schizophrenia -> PANSS total and Marder factors; QT studies -> time-averaged
QTcF change (ICH E14); renal -> composite sustained-eGFR-decline/ESRD/
renal-death endpoint; MS -> confirmed-disability-progression ADTTE plus
annualized relapse rate; infectious disease -> SVR12-style landmark
sustained-response flags; rare diseases -> cross-study pooling with
heterogeneous SDTM alignment.

Related standards: FDA Study Data Technical Conformance Guide (24 TAUGs
supported - regulatory weight for work items); SDTMIG oncology domains
TU/TR/RS; ADaM ADTTE/ADLB/ADQS structures; CDISC Biomedical Concepts /
CDISC 360i (machine-readable disease concepts - future source of formal
task specs).

## FDA and PHUSE

- **FDA Study Data Technical Conformance Guide** -
  https://Www.fda.gov/media/88173/download -
  Cross-cutting SDTM/SEND/ADaM expectations and traceability requirements.
  TA: cross-TA baseline. Grounding: "judge whether this ADaM package is
  submission-ready" validation-style benchmarks.

- **FDA TA-specific technical specifications**: Vaccines TS Guidance v2.1
  (https://www.fda.gov/media/112581/download), QT Studies TS v1.0
  (https://www.fda.gov/media/128187/download), HIV-1 clinical trial datasets
  (https://www.fda.gov/media/112667/download), bioequivalence ANDA analysis
  datasets (https://www.fda.gov/media/116187/download), NASH datasets
  including ADMI from SDTM MI (https://www.fda.gov/media/151864/download).
  Grounding: TA-flavored derivation work items (e.g. "build ADMI per FDA
  NASH specs") complementing CDISC TAUGs. QT Studies TS section 3.0 (ADEG):
  derived records "for the average of the individual replicate values ...
  at each nominal time point" with DTYPE "AVERAGE", PARAMCD at most 4
  characters, EGREPNUM as the replicate number, and Table 4's average of
  AVAL per PARAMCD. QT Studies TS comparator variables (Table 1: ACOMPFL, COMP,
  COMPBASE, COMPCHG, CCOMPCHG = CHG minus COMPCHG, COMPTYPE; Table 5: a
  parallel study's placebo group means by COMPTYPE "Placebo group at 0.5 h",
  shown on the drug and the placebo rows; appendix: in a parallel study with a
  nested crossover, the Day 14 placebo rows of both nested-crossover arms carry
  ACOMPFL "because the placebo is the comparator in this example", the baseline
  rows ABLFL) ground the placebo-corrected change
  (adam-adeg-placebo-corrected-change). The QT Studies TS pairs ADEG with ADPC:
  APERIOD, APERDAY, ATPT and ATPTN coded the same in both "so mapping of
  time-matched measures can be performed"; ECGPCFL "Y" for "a matching nominal
  PK sample time point for the nominal ECG time point" (Tables 7-8: a 12 h ECG
  without a sample, a 14 h sample without an ECG); ADPC AVAL from PCSTRESN with
  LLOQFL; CQTANLFL on drug and placebo days, never on baseline days (Table 9);
  placebo coded "Placebo" in TRTx (adam-adeg-time-matched-concentration). QT
  Studies TS Tables 1 and 6 (ADEG, ADPC) list the indexed analysis flags
  ANL01FL, ANL02FL and ANLzzFL ("Selection variable to identify the exact set
  of records used in the analysis number zz", controlled term Y) and TRTSEQP
  "Planned Sequence of Treatments"; the BE ANDA analysis datasets TS uses
  "ANL01FL = analysis with imputed scores" and "ANL02FL = analysis without
  imputed scores" (terms Y, N) and TRTSEQP in ADSL. Grounding: public source
  for indexed analysis flags and TRTSEQP in ADPP work items. The QT Studies
  TS ADEG tables (Table 1 variables; Tables 2 and 4 time-matched baseline
  days with BASETYPE "Baseline Day 1 h"; the appendix's per-period Day -1
  in a nested crossover) ground the ADEG baseline benchmarks
  (adam-adeg-time-matched-baseline).

- **PHUSE Test Data Factory (TDF)** - https://advance.phuse.global/display/WEL/Test+Dataset+Factory -
  data: https://github.com/phuse-org/phuse-scripts/blob/master/data/adam/TDF_ADaM_v1.0.zip
  and https://github.com/phuse-org/phuse-scripts/blob/master/data/sdtm/TDF_SDTM_v1.0%20.zip -
  Modernized CDISC pilot SDTM plus ADaM v1.1 with define.xml. TA: cross-TA.
  Grounding: ready-made "reproduce this ADaM build from SDTM plus metadata"
  work items with verifiable goldens.

- **PHUSE Define-XML v2.0 Completion Guidelines** -
  https://phuse.s3.eu-central-1.amazonaws.com/Deliverables/Optimizing+the+Use+of+Data+Standards/Define-XML+Version+2.0+Completion+Guidelines.pdf -
  Origin types, MethodDef usage, worked RFSTDTC/CMENDY/AWTDIFF examples.
  TA: cross-TA. Grounding: task-spec material for "write/validate define.xml
  metadata" benchmarks.

- **PHUSE Best Practices for Submission of Event Adjudication Data** -
  https://phuse.s3.eu-central-1.amazonaws.com/Deliverables/Optimizing+the+Use+of+Data+Standards/Best+Practices+for+Submission+of+Event+Adjudication+Data.pdf -
  Adjudicated events in SDTM (--LNKID plus RELREC) vs ADaM (analysis flags).
  TA: cross-TA. Grounding: "identify which assessments survive to ADaM"
  flag-derivation work items.

- **FDA guidance: Bioequivalence Studies With Pharmacokinetic Endpoints for
  Drugs Submitted Under an ANDA** (FDA, 2026) -
  https://www.fda.gov/media/192774/download -
  "If the pre-dose value is >5 percent of the Cmax, applicants should drop the
  subject of the affected period from BE study evaluations"; the test uses "the
  Cmax value for the subject in that period" and names no period; 12 to 18
  samples "including a pre-dose sample, per subject, per dose". TA: cross-TA
  (generics). Grounding: the 5 percent carry-over rule, per period, of
  adam-adpp-carryover-exclusion.

- **FDA guidance: Bioavailability Studies Submitted in NDAs or INDs -
  General Considerations** (FDA, 2022) -
  https://www.fda.gov/media/121311/download -
  Appendix A: pre-dose concentrations above 5 percent of Cmax, worded per
  subject, with "this subject's data should be flagged and reported"; III.A.7
  BA of the test product fasted and the effect of food. TA: cross-TA.
  Grounding: keeping excluded records in ADPP with a flag; the test-fasted
  treatment as test of one comparison and reference of the other.
  Multiple-dose studies dose to steady state and sample across the dosing
  interval at steady state; steady-state parameters AUC0-TAU, Cmax, Cmin,
  Ctrough: which profile of a multiple-dose study is the steady-state one
  (adam-adpp-accumulation-ratio).

- **FDA guidance: Statistical Approaches to Establishing Bioequivalence**
  (FDA, 2026) - https://www.fda.gov/media/163638/download -
  Missing data: complete case analysis ("removes subjects missing one or both
  periods") and available case analysis ("uses all observed data"),
  prespecified; III.H multiple references: "using only the data from those
  products". TA: cross-TA. Grounding: the complete-pair reading of
  adam-adpp-comparison-sets and its documented alternative.

- **FDA guidance: Assessing the Effects of Food on Drugs in INDs and NDAs**
  (FDA, 2026) - https://www.fda.gov/media/121313/download -
  Fed versus fasted crossover "with the fasted treatment arm serving as the
  reference"; a three-way crossover for two formulations; log-transformed AUC
  and Cmax. TA: cross-TA. Grounding: the food-effect comparison of
  adam-adpp-comparison-sets.

- **PHUSE FAQ-004: SDTM/ADaM IG Nuances** (PHUSE, living FAQ; the cited
  answer is dated 8 July 2020) -
  https://phuse.s3.eu-central-1.amazonaws.com/Deliverables/Optimizing+the+Use+of+Data+Standards/FAQ-004.pdf -
  "Storing a rounded value in AVAL is not good practice, as it typically
  results in a loss of precision for calculations in the tables."
  TA: cross-TA. Grounding: an unrounded AVAL on derived records, rounded
  in the listing or table program (adam-adpp-dose-normalized,
  adam-adpp-accumulation-ratio).

## ICH and EMA guidelines

- **ICH E14** (ICH, 2005) -
  https://database.ich.org/sites/default/files/E14_Guideline.pdf -
  Clinical evaluation of QT/QTc prolongation; section 3.1 lists the
  correction formulae, Fridericia's printed as QT/RR^0.33. TA: QT.
  Grounding: names the corrections; state the exact exponent in a prompt.
  Section 3.2.1 (the largest time-matched mean difference between drug and
  placebo, baseline-adjusted) grounds the by-time-point double delta
  (adam-adeg-placebo-corrected-change). Section 2.2.2: "Data on the drug
  concentrations around the time of ECG assessment would aid this assessment";
  3.2.3: "the relationship of drug concentrations to changes in QT/QTc
  interval": pairing each ECG time point with its concentration
  (adam-adeg-time-matched-concentration).

- **ICH E14/S7B Questions and Answers** (ICH, Step 4, 2022-02-21) -
  https://database.ich.org/sites/default/files/E14-S7B_QAs_Step4_2022_0221.pdf -
  Q&A 1.5: Fridericia's correction is appropriate in most situations;
  Q&A 4.2: "the average of the parameters from about 3 ECGs" within
  several minutes of each nominal time point. TA: QT. Grounding: replicate
  averaging and the correction choice in ADEG work items. Q&A 4.1 (in a
  placebo-controlled parallel design, drug and placebo means, each averaged
  across the study population, compared at each time point) and Q&A 5.1
  (concentration-QTc data from first-in-human and multiple ascending dose
  studies) ground one placebo group per time point for the whole study
  (adam-adeg-placebo-corrected-change). Q&A 4.2 also names the
  "time-matched" baseline (the same time points on the day before
  treatment as on the treatment day) and the "pre-dose" baseline, for
  parallel and crossover designs: which baseline an ECG time point takes
  (adam-adeg-time-matched-baseline).

- **EMA Guideline on the Investigation of Bioequivalence**
  (CPMP/EWP/QWP/1401/98 Rev. 1, 2010) -
  https://www.ema.europa.eu/en/documents/scientific-guideline/guideline-investigation-bioequivalence-rev1_en.pdf -
  4.1.7: "The lower limit of quantitation should be 1/20 of Cmax or lower".
  4.1.8: subjects without "evaluable data for both of the test and reference
  products" are not included; each comparison excludes the treatments "not
  relevant for the comparison in question"; Reasons for exclusion, item 2:
  "Subjects with non-zero baseline concentrations > 5% of Cmax"; Carry-over
  effects: "examination of the pre-treatment plasma concentrations in period 2
  (and beyond if applicable)", and a pre-dose concentration "greater than 5
  percent of the Cmax value for the subject in that period" excludes "the data
  from that subject for that period"; log transformation. TA: cross-TA.
  Grounding: both ADPP benchmarks; the period-2-and-beyond wording is the
  alternative to checking the first period too.

- **ICH M13A: Bioequivalence for Immediate-Release Solid Oral Dosage Forms**
  (ICH, Step 4, 2024) -
  https://database.ich.org/sites/default/files/ICH_M13A_Step4_Final_Guideline_2024_0723.pdf -
  2.2.2.2: a concentration below the LLOQ "should be treated as zero in PK
  parameter calculations"; 2.2.3.1: each comparison excludes "the data from the
  treatment arms that are not relevant for the comparison in question";
  2.2.3.2: "all subjects who provide evaluable data for both the test and
  comparator products"; 2.2.3.3: carry-over examined "in period 2 and beyond if
  applicable", then "excluding the data from that period". TA: cross-TA.
  Grounding: both ADPP benchmarks.
  For adam-adpp-dose-normalized: 2.1.6, dose proportionality "should consider
  Cmax and AUC as appropriate PK parameters" and compares the "dose-adjusted
  mean Cmax and AUC".

## ODM to SDTM mapping, define.xml, and ARM

- **REDCap2SDTM (J Biomed Inform, 2017)** -
  https://daneshyari.com/article/preview/4966880.pdf -
  EDC field annotations to SDTM metadata to dynamic dataset plus define.xml
  generation. TA: cross-TA. Grounding: "given ODM ItemDefs/annotations, map
  to SDTM domains/variables" work items; generate-define.xml-from-ODM tasks.

- **Maximizing CDISC ODM-XML for Specification of Submission Documentation
  (JSCDM)** - https://www.jscdm.org/article/170/galley/100/download/ -
  ODM-XML as executable spec generating aCRF/blank CRF/SDTM deliverables.
  TA: cross-TA. Grounding: "trace an SDTM variable back to its CRF item in
  ODM" work items.

- **CDISC SDTM/ADaM Pilot Project package and report** -
  inventory: https://github.com/yunbeom0405/cdisc-pilot-e2e/blob/HEAD/docs/DATA-INVENTORY.md -
  report: https://github.com/cdisc-org/sdtm-adam-pilot-project/raw/refs/heads/master/sdtmadampilotprojectreport.pdf -
  Canonical public SDTM to ADaM example (ADQSADAS, ADLBC/H/HY, ADTTE, ADAE
  with full MethodDefs; proto-ARM in define.xml). TA: neurology plus safety.
  Grounding: the most widely reused public ADaM build - "derive ADQSADAS
  from QS per the define.xml MethodDef" golden tasks.

- **atorus-research "04-define-arm-algorithms.md"** -
  https://github.com/atorus-research/cdisc_pilot_replication/blob/HEAD/notes/research/04-define-arm-algorithms.md -
  Table-by-table derivation reconstructions from the TDF define.xml (ACTOT
  missing-item adjustment, ANL01FL windowing, Hy's-law CRIT1FL/SHIFT1,
  ADTTE CNSR, LOCF/AVERAGE DTYPE). TA: cross-TA. Grounding: direct "trace
  derivation from define.xml" prompt material with documented edge cases
  (missingness, ties, windowing). Caveat encoded here: MethodDefs often
  defer to the SAP - encode that realistic incomplete-information condition
  in benchmarks rather than hiding it.

- **CDISC "Using R to generate Analysis Results Metadata (ARM)"
  (Japan Interchange, 2023)** -
  https://www.cdisc.org/sites/default/files/2023-07/2023_CDISC_Japan_Session7_AikHoe_Seah_2023-06-22%20(updated).pdf -
  Generating ARM sections of define.xml from an ADaM spec in R. TA:
  cross-TA. Grounding: "construct ARM linking table to datasets to
  where-clauses" work items for ADaM to results traceability.

## RConsortium Submissions Pilot 7 (synthetic data)

https://github.com/RConsortium/submissions-pilot7-synthetic-data

Fully synthetic end-to-end submission packages (no PHI): each study has
an enforced ODM -> SDTM -> ADaM -> TLF layout with define.xml, yamaa YAML
derivation specs, and official ADaM goldens verified cell-by-cell. The
closest thing to a ready-made benchmark pack for the pipeline.

- **ADADAS windowing + LOCF imputation (Alzheimer's)** -
  https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/submission-pilot3/spec/yamaa/adadas.yaml -
  ADAS-Cog BDS from SDTM QS: analysis windowing
  (AWTARGET/AWLO/AWHI/AWDIFF), 222 LOCF records at planned visits with no
  collected record, ANL01FL record selection, baseline/CHG/PCHG. Inputs:
  qs.parquet; mapping: aw_lookup.csv, plan.csv; golden: adadas.parquet
  (official). TA: neurology. Grounding: "reproduce this BDS build" golden
  task with a natural difficulty ladder (windowing -> LOCF -> baseline).

- **ADTTE time-to-event with event/censor sourcing (cross-TA pattern)** -
  https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/submission-pilot3/spec/yamaa/adtte.yaml -
  Event = first qualifying ADAE record (AOCC01FL='Y',
  CQ01NAM='DERMATOLOGIC EVENTS', SAFFL='Y', ordered by ASTDT); censor =
  disposition completion date with death override to RFENDTC; date
  imputation; SRCDOM/SRCVAR/SRCSEQ traceability; CNSR/EVNTDESC. TA:
  cross-TA. Grounding: the YAML documents a QC-observed rule explicitly
  absent from the define Derivation text - a ready-made "incomplete
  spec" condition for judge test vectors.

- **Fully verified 5-dataset ADaM pipeline (Alzheimer's)** -
  https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/submission-pilot5/spec/yamaa/ -
  ADSL, ADAE, ADADAS, ADTTE, ADLBC derived end-to-end and verified
  cell-by-cell against official FDA-submission ADaM (2,291,147/2,291,147
  cells at 1e-10 tolerance). Compare keys:
  https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/submission-pilot5/program/adam/adam-compare-keys.json -
  TA: neurology. Grounding: the compare-keys JSON + 1e-10 policy is a
  directly reusable judge/oracle pattern; the five verified specs give
  benchmark-sized task families from small (ADSL) to large (ADLBC).

- **Multi-instrument efficacy + Hy's law (neurology/safety)** -
  https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/cdiscpilot01/spec/yamaa/ -
  Three QS instruments in parallel (ADAS-Cog, CIBIC+, NPI-X) plus the lab
  safety chain (adlbc/adlbh/adlbhy, Hy's law); 8 yamaa specs, 10 official
  ADaM. TA: neurology + safety. Grounding: "combine" work-item seed
  (three QS benchmarks with near-identical structure); the define.xml
  here has zero MethodDefs - good for "trace derivation without
  MethodDef" variants.

- **ODM -> SDTM mapping (oncology)** -
  https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/kn189/data/odm/kn189_odm.tar.gz -
  KEYNOTE-189 (NSCLC, 616 subjects) and KEYNOTE-564 (adjuvant RCC, 994
  subjects) synthetic ODM CRF exports with full CRF metadata (RECIST RS,
  ECOG PS, PD-L1 biomarker, derived endpoints RE, subsequent therapy TT);
  21 ItemGroupDefs with NCT-linked OIDs. TA: oncology. Grounding: "map
  these ItemGroups to SDTM domains" tasks for the ODM end of the
  pipeline. Caveat: ODM only; no staged SDTM/ADaM yet.

- **Archive TA studies (CAR-T, RA, vasculitis, dermatology, PK)** -
  https://github.com/RConsortium/submissions-pilot7-synthetic-data/blob/main/.archieve/cart-t/ -
  R admiral-based reference programs (edc -> sdtm -> adam -> tlf) for five
  more therapeutic areas. TA: oncology (CAR-T), rheumatology, vasculitis,
  dermatology, PK. Grounding: new benchmark families (adce, adcm, adds,
  adie - datasets yamaa has no benchmarks for); cart-t's OpenClinica ODM
  export extends ODM -> SDTM mapping to cell therapy. Caveat: the archive
  is retained for reference, less curated than the active studies.

Pilot7-specific caveats: all data fully synthetic (realistic but not
real); submission-pilot6 ADaM parquets are pending (only SDTM +
define.xml + specs workbook staged); cdiscpilot01's define-adam.xml has
zero MethodDefs.

## Caveats to encode in work items

1. pharmaverseadam datasets are un-QC'd test data - verify each dataset
   before using it as golden truth.
2. define.xml MethodDefs frequently defer to the SAP - treat that as a
   realistic incomplete-information condition, not a spec defect.
3. Pin package and dataset versions in every work-item spec (see the
   mid-2026 reference points under pharmaverse).
4. Program-listing links above are not paper PDFs; check the official
   PharmaSUG program for the paper's current location before quoting it.
