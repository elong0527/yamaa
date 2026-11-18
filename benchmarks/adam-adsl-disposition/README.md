# Summarize End-of-Study and End-of-Treatment Disposition

**Goal:** give each subject a single end-of-study summary holding
`EOSDT`, `EOSREAS`, `EOSSTT`, and `DCSREAS`, plus an
independent end-of-treatment summary holding `EOTDT`, `EOTSTT`,
`DCTREAS`, and `DCTREASP`.

**Input:** demographics rows carrying the date of first study
treatment (`RFXSTDTC`), plus disposition records (`DS`) with
category (`DSCAT`), epoch (`EPOCH`), coded term (`DSDECOD`),
reported term (`DSTERM`), start date (`DSSTDTC`), and sequence
number (`DSSEQ`). Records with `EPOCH = 'TREATMENT'` describe
treatment disposition; the study summary reads every other epoch.

**Variables:**

- `EOSDT` is the start date (`DSSTDTC`) of the subject's last study
  disposition event: a record whose category (`DSCAT`) is
  `DISPOSITION EVENT` and whose epoch is not `TREATMENT`. It is
  empty when that event has no date or the subject has no such
  record.
- `EOSREAS` is the reported term (`DSTERM`) on that same last event.
- `EOSSTT` is `COMPLETED` when the last coded term (`DSDECOD`) is
  `COMPLETED`, `DISCONTINUED` for any other recorded term, and
  `ONGOING` when the subject has no study disposition event. It is
  empty for a screen failure (last coded term `SCREEN FAILURE`), who
  never entered the study.
- `DCSREAS` is the last coded term for a subject whose status is
  `DISCONTINUED`, naming the specific reason (for example
  `WITHDRAWAL BY SUBJECT`); empty otherwise, including for a screen
  failure.
- `EOTDT` is the start date of the subject's last treatment
  disposition event (`EPOCH = 'TREATMENT'`); empty when that event
  has no date or the subject has none.
- `EOTSTT` is `COMPLETED` when the last treatment coded term is
  `COMPLETED`, `DISCONTINUED` for any other recorded term, and
  `ONGOING` when the subject has no treatment disposition event. It
  is empty for a subject who never received study treatment (no
  `RFXSTDTC`), such as a screen failure.
- `DCTREAS` is the coded term on the last treatment event for a
  subject whose treatment status is `DISCONTINUED`; empty otherwise.
- `DCTREASP` is the reported term on that event when its coded term
  is `OTHER` - the specify text; empty otherwise.

**Note:** date, term, and reason come from a single last event, so
the three always describe the same event. The last event is the one
with the latest date, and when two events share that date, the
higher sequence number (`DSSEQ`) decides. An event without a date is
the last event only when the subject has no dated one, so an undated
withdrawal still makes the subject `DISCONTINUED`, with no end date.
Records in other categories, such as protocol milestones, never
count. The study and treatment tracks are independent: a subject who
stops treatment but completes the study is `DISCONTINUED` for
treatment and `COMPLETED` for study.
