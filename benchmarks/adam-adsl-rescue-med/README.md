# First Rescue Medication

**Goal:** flag whether each subject took rescue medication
(`RESCFL`), and give the start date of the first rescue medication
(`RESCDT`), its imputation flag (`RESCDTF`) and its standardized
name (`RESCTRT`).

**Input:** one subject-level table listing the subjects, plus
concomitant medication (`CM`) records carrying the reported name
(`CMTRT`), the standardized name (`CMDECOD`), a category (`CMCAT`),
a start date (`CMSTDTC`) collected in full or in part, and a
sequence number (`CMSEQ`).

**Variables:**

- `RESCFL` is `Y` when the subject has a record whose category
  (`CMCAT`) is `RESCUE MEDICATION`, and `N` otherwise, including a
  subject with no medication records at all.
- `RESCDT` is the start date of the first rescue medication: the one
  with the earliest start date, the smaller sequence number (`CMSEQ`)
  breaking a tie. A start collected as a year and month completes to
  the first of the month, and a year alone to 1 January, before the
  dates are compared. It is empty when the first rescue medication has
  no start date or the subject took none.
- `RESCDTF` is `D` when the day of `RESCDT` was imputed and `M` when
  the month and day were; empty when the date was collected in full or
  `RESCDT` is empty.
- `RESCTRT` is the standardized name (`CMDECOD`) of that first rescue
  medication; empty when the subject took none, never filled with
  placeholder text.

**Note:** a rescue record with no start date still makes the subject
`Y`, but it is the first rescue medication only when the subject has
no dated one. A duplicated record does not change the answer.
