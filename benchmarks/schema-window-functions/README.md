# Visit-Level Window Functions

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-window-functions.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** compute visit-level measures with every window expression over
one shared ordering: sequence numbering with a filter, competition and
dense ranking, reading a neighboring row, the last non-missing value, the
carried-forward value, and the baseline visit flag.

**Input:** `VS` carries one row per visit: subject, visit number
(`VISITN`), visit date (`VSDTC`), treatment start date (`TRTSDT`), the
result (`VSSTRESN`), and whether the visit is evaluable (`VSEVAL`), all
carried through unchanged.

**Windows:**

- `VSSEQ` numbers the evaluable visits from 1 within each subject in
  visit order; a visit that is not evaluable is left out before
  numbering and gets missing.
- `SEVRANKC` ranks each subject's results highest first with competition
  numbering: the two tied visits of subject `01` share rank 1 and the
  next value takes rank 3, skipping a position. `SEVRANKD` ranks the
  same ordering densely, so no number is skipped. Missing results rank
  after every present one and tie with each other, so the two missing
  results of subject `02` share one rank.
- `PREVRES` reads the previous visit's result and `NEXTRES` the next
  visit's. A missing result is not skipped: subject `01` visit 4 gets a
  missing previous result because visit 3's result is missing, just as
  a first visit does because nothing precedes it.
- `LASTRES` is the closest strictly earlier non-missing result, crossing
  any number of consecutive gaps: subject `02` visit 4 reads the visit 1
  result across two missing visits. The current visit is never a
  candidate, so a visit with a result still reads its predecessor's.
- `CARRYRES` returns the current result when present and otherwise the
  closest strictly earlier non-missing result. It differs from `LASTRES`
  only where the current visit has a result: a first visit with a result
  keeps it instead of reading missing.
- `BLFL` marks the latest visit dated on or before treatment start; two
  visits sharing that latest date stop the run rather than one being
  picked. The baseline visit can carry a missing result (subject `02`
  visit 2); a subject with no visit on or before treatment start
  (subject `04`) has no baseline.

**Note:** only `VSSEQ` leaves out the visits that are not evaluable.
Every other measure sees every visit, so a visit that is not evaluable
is still ranked, read as a neighbor, and can be the baseline visit.

**Standard:** ADaM | **Domain:** ADVS
