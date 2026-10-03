# Work item: enhance adam-adlb-lymphocytes to the full differential panel

## Work item

**Type:** enhance. **Target:** `adam-adlb-lymphocytes`.
**Change:** generalize the benchmark from lymphocytes alone to the
standard five-part white blood cell differential. Where the current
benchmark derives one absolute record (`LYMPH` = `WBC` x `LYMLE`)
per subject and visit, the enhanced benchmark derives one absolute
record per differential fraction present: `LYMLE` -> `LYMPH`,
`NEUTLE` -> `NEUT`, `MONOLE` -> `MONO`, `EOSLE` -> `EOS`,
`BASOLE` -> `BASO`. Every other rule is unchanged: one record per
subject, visit, and differential; only when a `WBC` count and the
fraction are both present; never when the absolute record already
exists; repeated contributing values stop the run via `ONLY()`;
`DTYPE` is `CALCULATION` on added records and empty on collected
ones. The golden gains rows (new derived records); no existing
golden value moves. The lifecycle badge resets to `draft`.

## What to measure

The fan-out pattern admiral's `derive_param_wbc_abs()` implements:
converting several lab differential fractions to absolute values in
one derivation (ADaM ADLB). The current benchmark pins the
`WBC x fraction` multiplication for a single differential, so a
solver can pass by special-casing `LYMPH`. The generalized
benchmark pins the same rule applied across the differential panel,
including that each differential is gated independently (a visit
with `WBC` and `NEUTLE` but no `LYMLE` still gains `NEUT`), that an
existing absolute record blocks only its own differential, and that
a visit with a fraction but no `WBC` gains nothing.

## Grounding material

- Existing benchmark to imitate: `adam-adlb-lymphocytes` itself
  (spec shape, README shape, prompt terseness); `adam-adlb-shift-criteria`
  (CRIT2 pilot) for the maintenance-protocol treatment of a generalized
  derivation.
- External reference: pharmaverse/admiral `derive_param_wbc_abs()`
  ("Adds a parameter for lab differentials converted to absolute
  values" -- admiral 1.5.0 reference index, r-packages.io). The
  derivation logic taken: for each subject and visit, each
  differential fraction present with a `WBC` count yields one
  absolute record equal to `WBC` times the fraction, flagged
  `CALCULATION`. Five-part differential mapping follows standard
  CDISC LB codes: LYMLE/Lymphocytes, NEUTLE/Neutrophils,
  MONOLE/Monocytes, EOSLE/Eosinophils, BASOLE/Basophils, with
  absolute codes LYMPH, NEUT, MONO, EOS, BASO.
- Example input data curated in `benchmarks/adam-adlb-lymphocytes/input/adlb.csv`;
  edge cases: a visit with `WBC` + `NEUTLE`/`MONOLE` but the
  differentials appear independently (LYM-001); a differential with
  `WBC` but no `LYMLE` (LYM-003 `NEUTLE`); a fraction with no `WBC`
  (LYM-004 `EOSLE`); an existing absolute record blocking only its
  own differential (LYM-005 `MONO`); a fresh subject exercising
  `EOS`/`BASO` (LYM-006). All fixture fractions are exact binary
  fractions (0.5, 0.75, 0.25, 0.125) so golden products are exact.

## Difficulty target

1. The solver must fan out over an open set of differentials
   rather than hard-coding `LYMPH`; the prompt names the mapping,
   not the fixtures' rows.
2. Independent gating: `NEUT` is derived for LYM-003 even though no
   `LYMLE` exists there.
3. The existing-record block is per-differential: LYM-005's existing
   `MONO` blocks `MONO` but the existing `LYMPH` does not block
   other differentials.
4. A fraction without `WBC` (LYM-004 `EOSLE`) derives nothing.

## Out of scope

- Unit handling or fraction-vs-percent conversion (all fixtures are
  fractions of 1). The unit question is a separate benchmark.
- More than the five standard differentials (e.g. immature
  granulocytes). The directory name `adam-adlb-lymphocytes` is kept
  for leaderboard continuity; the README title generalizes.

## Acceptance sketch

1. `factory.sh validate adam-adlb-lymphocytes` clean; drift-check
   passes; every pre-existing golden value byte-identical.
2. The oracle (R and Python reference solutions from prompt +
   inputs alone) scores 1 on the new golden.
3. The Harbor prompt still asks for exactly the golden's datasets
   and columns; `conventions.md` within its 20-line prompt-tier
   limit; `brief.md` rebuilt via `evaluations/harbor/brief.py`.
