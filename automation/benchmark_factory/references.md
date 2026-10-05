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

## CDISC standards

- **ADaM Basic Data Structure for Time-to-Event Analyses v1.0** (CDISC, 2012) -
  https://www.cdisc.org/standards/foundational/adam -
  Time-to-Event example (p. 19): PFS records with CNSR/EVNTDESC/CNSDTDSC,
  including "NEW ANTI-CANCER THERAPY. CENSORED AT TIME OF LAST ASSESSMENT."
  and "NO BASELINE ASSESSMENT. CENSORED AT TIME OF RANDOMIZATION."
  TA: cross-TA. Grounding: canonical PFS censoring conventions and
  censoring-reason descriptions for ADTTE benchmarks.

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

Pin package versions in work-item specs. Mid-2026 reference points:
admiral 1.5.0, admiralonco 1.4.1, admiralophtha 1.5.0, admiralvaccine 0.6.0,
admiralpeds 0.4.0, admiralmetabolic 0.3.0, admiralneuro 0.3.0,
pharmaversesdtm 1.5.0.

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
  NASH specs") complementing CDISC TAUGs.

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
