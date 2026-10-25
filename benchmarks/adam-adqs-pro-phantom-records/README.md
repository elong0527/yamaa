# Phantom Records for Missed PRO Assessments

**Goal:** following the FDA technical specification for patient-reported
outcome (PRO) data in cancer trials, give every randomized subject a record
for each item and the total score of a two-item fatigue measure at each
planned assessment, adding a phantom record for each one not collected, with
the analysis value (`AVAL`), the record type (`DTYPE`), the analysis reason
not performed (`AREASND`), the PRO expected flags for clinical benefit
(`PROEX1FL`) and for safety and tolerability (`PROEX2FL`), and the score
completed flag (`PROSCMFL`).

**Input:** one subject-level record per randomized subject with the
randomization date, the safety population flag, the date and reason
treatment ended, and the death date; the planned PRO schedule, one record per
planned timepoint and parameter, with its planned day counted from
randomization as day 1; and the collected questionnaire item records, each at
its planned visit, holding a response on a 0 to 4 scale or a not-done status
with its reason.

**Variables:**

- `PARCAT2`, `PARAMCD`, `PARAM`, `AVISIT` and `AVISITN`: the planned
  parameter, `ITEM` or `TOTAL SCORE` in `PARCAT2`, and the planned visit,
  from the schedule.
- `AVAL`: the item's response; on the total score (`FATTOT`), the sum of both
  items, empty unless both were answered.
- `DTYPE`: `PHANTOM` on an item record the questionnaire data does not hold,
  and on the total score at a timepoint where neither item was collected;
  empty otherwise.
- `QSSTAT` and `QSREASND`: the collected item's completion status and reason
  not done, empty on a phantom record and on the total score.
- `AREASND`: empty when `AVAL` has a value. On a phantom record, `NOT
  TREATED` for a subject outside the safety population, the reason treatment
  ended at a timepoint planned after the end of treatment, and empty
  otherwise. On a collected item, its reason not done; on the total score,
  the reason its items carry, or `NOT CALCULABLE` when they carry none.
- `PROEX1FL`: `Y` at every timepoint not planned after the subject's death.
- `PROEX2FL`: `Y` for a subject in the safety population at a timepoint
  planned after neither the end of treatment nor death; empty otherwise.
- `PROSCMFL`: `Y` when `AVAL` has a value.

**Note:** a timepoint is planned after an event when its planned day is
later than the day of the event, both counted from randomization as day 1,
so a timepoint planned on the day of the event is still expected. A subject
whose treatment has not ended is expected for safety at every timepoint
before death, and records collected after the end of treatment are kept.
