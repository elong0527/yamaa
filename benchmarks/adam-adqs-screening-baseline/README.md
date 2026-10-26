# Screening Score Used as Baseline

**Goal:** flag each subject's baseline pain score (`ABLFL`), mark a baseline
taken from the screening questionnaire instead of the protocol's baseline
visit (`SCBLFL`), and carry the baseline score (`BASE`) and the change from
it (`CHG`), alongside each subject's treatment start date (`TRTSDT`).

**Input:** collected questionnaire records of a single-item worst-pain rating
from 0 to 10, at screening, Cycle 1 Day 1 and Cycle 2 Day 1, each with its
visit number (`VISITNUM`), completion date and numeric result, which is
empty when the questionnaire was not done; plus subject-level treatment
start dates.

**Variables:**

- `ADT` is the questionnaire record's collection date, kept when the
  questionnaire was not done; empty when the visit was not attended.
- `TRTSDT` is the subject's treatment start date.
- `AVAL` is the collected rating; empty when the questionnaire was not done.
- `ABLFL` is `Y` on the Cycle 1 Day 1 record when it has a rating and was
  completed on or before the treatment start date. When it has no rating or
  was completed after the first dose, `ABLFL` is `Y` on the screening record
  instead, if that has a rating. Blank on every other record, and on every
  record of a subject with neither.
- `SCBLFL` (Screening Used as Baseline Flag) is `Y` on the baseline record
  when it is the screening record; blank on every other record, including a
  Cycle 1 Day 1 baseline.
- `BASE` is the baseline rating, repeated on every record of the subject
  and parameter; empty when the subject has no baseline.
- `CHG` is `AVAL` minus `BASE` on each record completed after the treatment
  start date; empty on the baseline record and every other record on or
  before that date, and wherever `AVAL` or `BASE` is empty.

**Note:** the protocol schedules the baseline questionnaire on Cycle 1 Day
1, before the first dose, and only it or the screening questionnaire can be
the baseline, so a later rating never stands in. The screening record
itself carries both flags; no separate baseline record is added.
