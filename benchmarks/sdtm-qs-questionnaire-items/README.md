# Questionnaire Items

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-qs-questionnaire-items.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build `QSSEQ`, `QSTESTCD`, `QSTEST`, `QSCAT`,
`QSORRES`, `QSSTRESC`, `QSSTRESN`, `QSSTAT`, `QSREASND`,
`QSBLFL`, `QSDRVFL`, `VISITNUM`, and `QSDTC` for the nine
PHQ-9 items at two visits for two subjects, plus a total-score
record for each fully answered questionnaire, in
Questionnaires (QS).

**Input:** `input/odm.csv` is long-form ODM item data: one row
per collected item, identified by study (`StudyOID`), subject
(`SubjectKey`), visit (`StudyEventOID`), item group, and item
(`ItemOID`), with the circled score (0 through 3) in `Value`.
`input/visits.csv` gives the visit number and collection date
for each subject visit. `input/notdone.csv` declares what was
not collected, with the reason: one logically skipped item
(its own item row) and one whole questionnaire the subject
refused (a single row with no item, standing for the
questionnaire). `input/items.csv`
lists the nine instrument items with their short names and
full text.

**Variables:**

- `QSTESTCD` is `PHQ901` through `PHQ909` for the nine items,
  `PHQ9T` for the total-score record, and `QSALL` for the refused
  questionnaire. These short names are
  fixture-local labels for this benchmark, not verified CDISC
  controlled terminology.
- `QSTEST` is the item text; the total record reads
  "Patient Health Questionnaire 9 item total score"; the
  refused-questionnaire record reads "All Questionnaires".
- `QSCAT` is always `PHQ-9`.
- `QSORRES` is the response text matching the circled score
  ("Not at all", "Several days", "More than half the days",
  "Nearly every day"). The total-score record has no original
  result because the score is computed, not collected, so
  `QSORRES` stays missing on it.
- `QSSTRESC` is the collected score as text, or the total as
  text on the total record.
- `QSSTRESN` is the collected score as a number, or the total
  as a number.
- `QSSTAT` is `NOT DONE` on the skipped-item record and on the
  refused-questionnaire record; `QSREASND` says why
  (`LOGICALLY SKIPPED ITEM` or `SUBJECT REFUSED`).
- Records with `QSSTAT = 'NOT DONE'` carry no result:
  `QSORRES`, `QSSTRESC`, and `QSSTRESN` stay empty.
- The total-score record appears only for a visit where all
  nine items were answered; a visit with a skipped or refused
  item gets no total.
- `QSDRVFL` is `Y` on the total-score record only.
- `QSBLFL` is `Y` on every record from the first visit.
- `QSDTC` is the date the questionnaire was collected.
- `QSSEQ` numbers the subject's records by visit, then by test
  short name, so the total-score record (`PHQ9T`) sorts after
  the nine items, and the refused-questionnaire record
  (`QSALL`) sorts last, alphabetically.

**Note:** the logically skipped item keeps its record with
`QSSTAT = 'NOT DONE'` instead of disappearing, following the
FDA convention for items the instrument instructions skip; the
refused questionnaire keeps one record instead of nine, with
`QSTESTCD = 'QSALL'`, `QSSTAT = 'NOT DONE'`, and
`QSREASND = 'SUBJECT REFUSED'`, per issue #374's design.

**Standard:** SDTM | **Domain:** QS
