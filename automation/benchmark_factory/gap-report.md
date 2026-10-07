# Benchmark gap report

Sourced proposals for growing and pruning the yamaa benchmark suite.
Surveyed 2026-10-02; every URL below was verified live that day.

## Sources

- **pharmaverse/admiral function reference**
  (https://pharmaverse.github.io/admiral/reference/) -- the richest
  public catalog of ADaM derivations. Each candidate below names the
  admiral function whose derivation it mirrors.
- **CDISC SDTMIG v3.3 domain table**
  (https://www.cdisc.org/standards/foundational/sdtmig/sdtmig-v3-3/html)
  -- for the missing SDTM domains (EG, DV, PC).
- **GxP-Agent / CDISC-Bench** (https://arxiv.org/pdf/2608.16890) -- an
  execution-based benchmark built from the FDA pilot submission
  CDISCPilot01 (254 subjects, 49 ground-truth ADSL variables; ADAE
  with 55 variables and 1,191 records). Complementary to yamaa, not
  competing: it tests whole-dataset synthesis (where the paper
  reports 0% single-shot success across five frontier models), while
  yamaa tests small derivation tasks. No existing benchmark covers
  yamaa's niche (unit-sized CDISC derivation tasks for agents).
  CDISCPilot01 is public FDA data and a candidate source of
  realistic derivation patterns.
- **ASA Biopharm AI/ML Working Group benchmark**
  (github.com/doublerobust/asabiop-aiml-agentic) -- TFL programming,
  18 test cases. Different layer (tables/listings), noted for
  completeness.

## A. New benchmark candidates

Starred items are the highest priority: realistic, non-trivial, and
absent from the suite. Each is one factory run: fill in `task-spec.md`
with the derivation below and the admiral function as grounding.

### High priority

1. **`adam-adlb-egfr`** * -- derive eGFR (CKD-EPI equation) as a
   computed lab parameter from creatinine, age, sex, and race.
   Piecewise formula with sex/race coefficients and unit handling;
   a standard safety parameter. Grounding: `compute_egfr()`.
2. **`adam-adlb-hys-law`** * -- flag subjects meeting Hy's law
   criteria for drug-induced liver injury (ALT >= 3xULN with
   bilirubin >= 2xULN and ALP < 2xULN at the same visit). Classic
   FDA safety derivation. Grounding: `derive_vars_crit_flag()`.
3. **`adam-adlb-limit-imputation`** * -- impute numeric analysis
   values from character results carrying `<` or `>` qualifiers
   (e.g. `"<0.5"` becomes half the limit). The positive counterpart
   to `negative-number-below-limit`. Grounding:
   `compute_qual_imputation()`.
4. **`adam-adlb-toxicity-grade`** * -- derive CTCAE toxicity grades
   (`ATOXGR`) for lab parameters against grading metadata.
   Grounding: `derive_var_atoxgr()`. Check `sdtm-lb-grading` first:
   if it already covers CTCAE grading, this may collapse into an
   enhancement of that benchmark instead.
5. **`adam-adex-dose-intensity`** * -- derive dose intensity (actual
   vs planned) per subject and period. Distinct from
   `adam-adex-dose-reduction` (a flag) and
   `adam-adex-cumulative-dose` (a total). Grounding:
   `derive_param_doseint()`.

### Further candidates

6. **`adam-adlb-framingham`** -- Framingham 10-year cardiovascular
   risk score from age, sex, blood pressure, smoking, diabetes, and
   cholesterol. A non-trivial multi-input computed parameter.
   Grounding: `compute_framingham()`.
7. **`adam-adeg-qtc-flags`** -- flag ECG records meeting cardiac
   safety outlier criteria (QTcF > 450/480/500 ms, change from
   baseline > 30/60 ms). `adam-adeg-derived-intervals` derives the
   values; nothing flags the outliers. Grounding:
   `derive_vars_crit_flag()` pattern.
8. **`adam-adcm-prior-concomitant`** -- classify each medication as
   prior, concomitant, or both relative to treatment start.
   Standard ADaMIG derivation; grounding: admiral OCCDS derivations
   (`derive_var_trtemfl()` family).
9. **`adam-admh-history-flags`** -- build ADMH occurrence flags from
   MH (by body system, active-at-study-start). No ADMH benchmarks
   exist at all. Grounding: admiral OCCDS derivations.
10. **`adam-adtr-percent-change-nadir`** -- percent change from nadir
    for target lesions, the standard next step after
    `adam-adtr-nadir` (running nadir) and `adam-adtr-sum`.
    Grounding: standard RECIST practice per admiralonco (verify the
    exact function page when writing the task spec).
11. **`sdtm-eg-ecg-findings`** -- SDTM EG mapping with replicates
    and timepoint references. No EG coverage exists despite ADEG
    benchmarks downstream. Source: SDTMIG v3.3 EG domain (URL
    above).
12. **`sdtm-dv-protocol-deviations`** -- SDTM DV mapping plus
    important-deviation flagging. No DV coverage exists. Source:
    SDTMIG v3.3 DV domain (URL above). *Uncertainty: "important"
    flag criteria are sponsor-specific; pin them in fixtures.*
13. **`sdtm-pc-pk-concentrations`** -- SDTM PC mapping with nominal
    timepoint parsing (PCTPT). No PK coverage exists anywhere in
    the suite. Source: SDTMIG v3.3 PC domain (URL above); admiral
    `convert_xxtpt_to_hours()`. *Uncertainty: PK is a whole
    subfield -- start with PC mapping only; ADPP parameter
    derivation (Cmax/Tmax/AUC) is a follow-up, not this ticket.*

## B. Enhance candidates

1. **`adam-adlb-shift-criteria`** ("classify a result, its shift from
   baseline, and one criterion") -- extend from a single criterion
   to two (CRIT2/CRIT2FL, a low-tail criterion against LLN),
   exercising the multi-criterion pattern admiral's
   `derive_vars_crit_flag()` supports. Keep backward-compatible
   with the existing golden: existing values do not move. *Status:
   chosen as the factory's first maintenance pilot; see
   work-item issue #1609.*
2. **`adam-adlb-lymphocytes`** -- verify against
   `derive_param_wbc_abs()` ("lab differentials converted to
   absolute values"); if it matches, generalize the benchmark to
   all differentials rather than lymphocytes alone.
3. **`adam-advs-windows`** -- add expected/missing visit records
   (cf. `derive_expected_records()`): planned visits with no
   collected result are a realistic gap in the current fixtures.
   Same question applies to `adam-adqs-missed-visit-locf`; inspect
   both fixtures before deciding.

   > REVIEWED 2026-10-07 (benchmark-maintenance run) -- candidate
   > COMPLETED. `adam-advs-windows` already carries expected records:
   > one per planned visit in the screening, baseline, week 2, or week 4
   > windows for which no collected record's study day covers its window,
   > driven from SDTM SV (6af7cbc9, 303efd5a; drift and
   > the 19-row golden verified 2026-10-07). `adam-adqs-missed-visit-locf`
   > inspected per the "same question": missed visits already produce
   > exactly one record per efficacy subject per scheduled visit via
   > stated protocol row templates (deliberate design, spec header
   > comment); no change needed.

Dropped from this list: `adam-adlb-bds` already carries PCHG
(verified in the golden), and ANRIND is covered by
`adam-adlb-shift-criteria`.

## C. Combine candidates

All overlap judgments below are from names and index descriptions
only; verify the benchmark contents before merging. The repo's
`spec_<variant>.yaml` convention exists for exactly this: one
benchmark, parametric variants.

1. **`adam-advs-carryforward`** + `adam-advs-first-observed-carry` +
   **`adam-advs-prior-result`** -- three benchmarks about carrying a
   value across visits ("once-measured characteristic" / "first
   observed result" / "latest earlier character result"). Strong
   overlap; merge into one benchmark with variants.

   > REVIEWED 2026-10-05 (benchmark-maintenance run) -- candidate
   > REJECTED. Contents verified against all three specs: the
   > overlap is surface-level (all use a look-back across visits),
   > but each pins a different derivation pattern. `carryforward`
   > (plan-spine LOCF into AVAL plus the HGTBLFL/HEIGHTBL
   > baseline-height flag) is the only one joining planned
   > measurements to collected records on test code and date; the
   > baseline-height flag has no counterpart in the others.
   > `first-observed-carry` derives BASEVAL by rank-1 selection
   > over an observed-only window and carrying it to every later
   > visit (first-observed-as-baseline semantics), mechanically
   > different from LOCF's previous-non-missing fill. `prior-result`
   > operates on character results grouped by subject and series,
   > deriving PREVAVALC as an audit column. Merging into parametric
   > variants would lose this coverage; keep all three.
2. **`adam-advs-locf`** + **`adam-advs-locf-record`** -- "carry the
   last observed value across missing visits" vs "carry one observed
   record to each planned visit". Likely variant-level differences
   in one LOCF pattern. Keep `adam-adqs-missed-visit-locf`
   separate: different domain, different application.

   > REVIEWED 2026-10-05 (benchmark-maintenance run) -- candidate
   > REJECTED. Contents verified against both specs: the two are
   > distinct derivation patterns, not variant-level differences.
   > `adam-advs-locf` fills a single column in place (`locf` on
   > AVALCOL within the VS table; no earlier value stays missing,
   > zero is a value). `adam-advs-locf-record` is plan-spine as-of
   > donor selection: the latest ANL01FL='Y' observation with a
   > non-null AVAL at or before each planned visit (AVISITN then
   > QSSEQ tie-break), carrying the AVAL/ADT/QSSEQ record bundle
   > together, with a no-donor-leaves-all-missing `implies` check.
   > The plan-spine driver, the donor-selection mechanic, and the
   > multi-column record carry have no counterpart in
   > `adam-advs-locf`. Merging into parametric variants would lose
   > this coverage; keep both.
3. **`adam-adlb-row-window`** + **`adam-adlb-window-chain`** -- "lag
   a result across visits while constructing change parameters" vs
   "carry a previous visit's change". Both pin previous-visit
   mechanics in ADLBC; check `schema-window-*` coverage, then merge
   if the difference is parametric.

   > REVIEWED 2026-10-04 (benchmark-maintenance run) -- candidate
   > REJECTED. The actual benchmark names are
   > `adam-adlbc-row-window` and `adam-adlbc-window-chain` (the names
   > above are stale). Contents verified against both specs: the
   > difference is mechanical, not parametric. `window-chain` is the
   > only positive benchmark in the suite whose `row_value` reads a derived
   > column (PREV2 = the previous visit's CHG, read after CHG is
   > complete for every row); neither `schema-window-functions` (all
   > its `row_value` sources are input columns) nor
   > `schema-window-intermediate-rank` (no `row_value` at all) covers
   > this chained-lag pattern. `row-window` additionally pins
   > multi-parameter isolation (ALB+BILI -> _ALB/_BILI with the window
   > grouped by USUBJID+PARAMCD, so an _ALB row never reads a _BILI
   > value). Merging would lose coverage; keep both benchmarks.
4. **`adam-adae-severity-rank`** + **`adam-adae-worst-severity`** --
   "rank a subject's events by severity" vs "flag the worst-severity
   event per preferred term". Ranking subsumes worst-flagging;
   verify whether severity-rank covers the per-PT grouping before
   retiring worst-severity.

## D. Retire / deprioritize candidates

Conservative list; retirement is a human Stage 5 decision, never an
agent's.

Correction (2026-10-02): `adam-adsl-investigator-comment` was listed
here as a trivial copy, but its fixtures test comma, quote, and
line-break fidelity plus empty-vs-spaces handling -- real CSV edge
cases. Removed from the retire list; it stays.

1. **`adam-adsl-site-parse`** -- "parse the site from the subject
   identifier": a string-parsing exercise rather than a CDISC
   derivation. Realistic (sites do encode site ID in USUBJID), so
   "deprioritize" is fairer than "retire".
2. ~~**`adam-adae-post-covid`** -- the COVID-specific framing is dated,
   but the underlying pattern (flag events after a reference event)
   is generic and worth keeping. Generalize/repurpose rather than
   delete.~~ Resolved by work-item issue #1627: renamed to
   `adam-adae-post-index` (`AFTIDXFL`, "After Index Event Flag");
   fixture and golden values retained.

## E. Pipeline integration candidates

Not pattern benchmarks: end-to-end chains that prove the stages plug
together, each with per-stage verified goldens (the pilot7 model:
ODM -> SDTM -> ADaM with a golden at every stage). These catch
interface mismatches the unit benchmarks cannot, and they demo the
product story. Keep them few and clearly labeled; they complement,
never replace, the stage-separated benchmarks.

1. **`pipe-vs-advs`** -- ODM vital-signs form -> SDTM VS -> ADVS with
   analysis windows and expected records. Reuses the `sdtm-vs-*`
   mapping patterns and the `adam-advs-windows` derivation; the new
   work is the handoff (VSDTC/VSDY/VSSTRESN into ADT/ADY/AVAL).
   Grounding: pilot7 `submission-pilot3` (define + yamaa specs).
2. **`pipe-lb-adlb`** -- ODM lab form -> SDTM LB -> ADLB with baseline,
   change, and shift criteria. Reuses `sdtm-lb-*` and the
   `adam-adlb-shift-criteria` derivation.
3. **`pipe-dm-adsl`** -- ODM demographics form -> SDTM DM -> ADSL with
   treatment dates and population flags. The smallest chain; a good
   first one.

Each is one `create` run with two goldens (the SDTM stage and the
ADaM stage); a stage failure must be diagnosable to its stage, so
keep the per-stage specs in separate files even when one benchmark
directory holds them.

## Uncertainties

- Combine candidates (C): overlap assessed from names and one-line
  descriptions only. Confirm from contents before acting.
- `sdtm-lb-grading` scope (normal-range vs CTCAE) determines
  whether `adam-adlb-toxicity-grade` is new or an enhancement.
- admiralonco citation for percent-change-from-nadir: standard
  RECIST practice; pin the exact function page at task-spec time.
- `adam-adcm-prior-concomitant`: standard ADaMIG derivation;
  grounding cited via the admiral OCCDS section rather than a
  dedicated function page.

## Using this report with the factory

Each item in A is one run: fill `task-spec.md` (the derivation
above becomes "What to measure"; the admiral function becomes
grounding material), curate the fixtures, and start the loop. Items
in B are revision tasks on existing benchmarks (loop Stages 1-4
with the benchmark as the starting checkpoint). Items in C and D
are proposals for the human gate, not for the agent.
