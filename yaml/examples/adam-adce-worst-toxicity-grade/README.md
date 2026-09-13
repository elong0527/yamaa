# Flag the worst-graded event per subject

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adce-worst-toxicity-grade.html)

**Goal:** rank severity and flag the worst-graded clinical event
(CE) for each subject, adding `ASEVN`, `ATOXGRN` and `AOCCFL`.

**Input:** clinical event records carrying sequence number
(`CESEQ`), reported term (`CETERM`), collected severity
(`CESEV`), and analysis start date (`ASTDT`).

**Variables:**

- `ASEV`: analysis severity, carrying the collected severity
  (`CESEV`); blank when none was collected.
- `ASEVN`: numeric rank of `ASEV`, `1` for MILD through `3` for
  SEVERE; blank when no severity was collected.
- `ATOXGRN`: toxicity grade, equal to `ASEVN`, so a moderate
  event reads grade `2`; blank when no severity was collected.
- `AOCCFL`: `Y` on the graded event with the greatest `ATOXGRN`
  for the subject; blank otherwise. Ties break by earliest
  `ASTDT`, then lowest `CESEQ`, so at most one event per subject
  is flagged.

**Note:** an event without a grade can never be flagged, so a
subject with no graded event has no flagged event. Grading stays
separate from flagging so the grade means the same thing on
every event while the flag answers a question about the subject.

**Standard:** ADaM | **Domain:** ADCE
