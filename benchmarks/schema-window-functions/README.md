# Visit-Level Window Functions

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-window-functions.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** compute visit-level measures with every window expression over
one shared ordering: sequence numbering with a filter, competition and
dense ranking, reading a neighboring row, the last non-missing value, the
carried-forward value, and the baseline visit flag.

**Input:** `VS` carries one row per visit: subject, visit number, visit
date, treatment start date, the result, and whether the visit is
evaluable. The visit number (`VISITN`), visit date (`VSDTC`), treatment
start date (`TRTSDT`), result (`VSSTRESN`), and evaluable flag (`VSEVAL`)
are carried through unchanged. Subject `01` has five visits, one not
evaluable with a missing result. Subject `02` has two consecutive visits
with missing results. Subject `03` has two tied results. Subject `04` has
a single visit after treatment start, so it has no baseline.

**Windows:**

- `VSSEQ` numbers the evaluable visits from 1 within each subject; the
  non-evaluable visits are excluded before numbering and receive missing.
- `SEVRANKC` ranks the results highest first with competition numbering:
  the two tied visits of subject `01` share rank 1 and the next value
  takes rank 3, skipping a position. `SEVRANKD` ranks the same ordering
  densely, so no number is skipped. Two missing results are equal for
  ranking, whatever `nulls: last` places them among.
- `PREVRES` reads the previous visit's result and `NEXTRES` the next
  visit's. A present row with a missing value reads the same as an absent
  row: subject `01` visit 4 reads its previous result as missing because
  visit 3's result is missing.
- `LASTRES` is the closest strictly earlier non-missing result, crossing
  any number of consecutive gaps: subject `02` visit 4 reads the visit 1
  result across two missing visits. The current row is never a candidate,
  so a present value still reads its predecessor.
- `CARRYRES` returns the current result when present and otherwise the
  closest strictly earlier non-missing result. It differs from `LASTRES`
  only where the current row has a value: the first visit keeps its own
  result instead of reading missing.
- `BLFL` marks the unique latest visit dated on or before treatment
  start. The baseline visit can carry a missing result (subject `02`
  visit 2); a subject with no visit on or before treatment start
  (subject `04`) has no baseline.

**Note:** filtering happens before partitioning, so excluded rows receive
missing from every window expression, not a zero or a carried value.

**Standard:** ADaM | **Domain:** ADVS
