# Work item: shift-criteria-crit2

## Work item

**Type:** enhance. **Target:** `benchmarks/adam-adlb-shift-criteria`
(reviewed). **Change:** add a second criterion pair, CRIT2/CRIT2FL,
assessing the low tail ("Result less than LLN"), mirroring the
existing CRIT1/CRIT1FL structure. Nothing else changes.

## What to measure

The multi-criterion pattern from admiral's `derive_vars_crit_flag()`:
two independent criterion texts with independent flags on the same
records. CRIT2 states "Result less than LLN"; CRIT2FL is Y when
`AVAL < ANRLO`, N when assessable but not met, empty when `AVAL` or
`ANRLO` is missing. Strictly less than: a result exactly at the
lower limit does not meet it. The two criteria have independent
missingness: a record can have CRIT1FL empty (upper limit missing)
while CRIT2FL is assessable.

## Grounding material

- The existing benchmark itself: `benchmarks/adam-adlb-shift-criteria`
  (spec, fixtures, golden, README, prompt). Imitate its CRIT1
  construction exactly.
- admiral `derive_vars_crit_flag()`
  (https://pharmaverse.github.io/admiral/reference/): CRITy /
  CRITyFL / CRITyFN pattern.
- Curated fixture additions (2 rows):
  - EOS, WEEK 8, AVAL 0.5, limits 1-6: below LLN, so CRIT2FL Y.
    Exercises the Y case on the low tail.
  - GGT, WEEK 4, AVAL 5, ANRLO 10, ANRHI missing: CRIT1FL empty
    (upper limit missing) while CRIT2FL is Y. Exercises
    independent missingness.
  - Boundary N (AVAL exactly at ANRLO) and empty cases (missing
    AVAL, missing ANRLO) already exist in the fixtures.

## Difficulty target

1. The strict inequality at the boundary: AVAL = ANRLO reads N,
   not Y (mirrors CRIT1's strict `>`).
2. Independent missingness: CRIT2FL assessable exactly when AVAL
   and ANRLO are present, regardless of ANRHI.
3. The criterion text and its flag always arrive together
   (all_or_none verification, as for CRIT1).

## Out of scope

- No change to any existing column, value, or verification.
- No new parameters or subjects beyond the two fixture rows.
- CRIT1 semantics untouched.

## Acceptance sketch

- spec.yaml gains CRIT2/CRIT2FL (+ verifications); output.columns
  extended; row_count updated 15 -> 17.
- input gains 2 rows; golden gains 2 rows and 2 columns; every
  pre-existing golden value byte-identical (checked with git diff).
- README gains the two variable bullets; lifecycle badge reset to
  draft.
- Harbor prompt (full.md, conventions.md, brief.md) asks for the
  new columns with the same rules.
- `./factory.sh validate adam-adlb-shift-criteria` clean;
  drift-check (pytest -k) passes.

## Stage 4 judge review (2026-10-02)

Verdict: **accept** -- ready for the human gate.

- Construct validity: pass. Multi-criterion CRITy/CRITyFL flags are
  the admiral `derive_vars_crit_flag()` pattern; distinct from the
  old single-criterion task.
- Correctness: pass. Engine reproduces the golden exactly
  (drift-check). Golden derived independently from the rules:
  Y only under strict `AVAL < ANRLO`; the boundary row
  (AVAL = ANRLO) reads N; missing AVAL/ANRLO rows are empty.
- Feasibility: pass. Full prompt states both criteria as rules;
  a reference solution follows from prompt + inputs alone.
- Usefulness: pass. Closes gap-report B.1; adds strict-boundary
  and independent-missingness discrimination.
- No leakage: pass. Prompt names no spec vocabulary; README
  contract validated clean.
- Maintenance checks: no silent regression (all 15 pre-existing
  golden rows byte-identical on original columns, verified
  programmatically); no coverage loss (additive only); prompt
  parity (full/conventions/brief all updated); lifecycle reset
  to draft.
