# Window Rank Over Intermediate Records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-window-intermediate-rank.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** rank each subject's adverse events by severity, worst first.

**Input:** `spec.yaml` builds one record per row of `input/ae.csv`,
which carries the subject, the event sequence number, the coded term,
the reported severity, and the onset date.

**Variables:**

- `AEDECOD` holds the coded adverse event term, copied from the
  source record.
- `AESEV` holds the reported severity (MILD, MODERATE, or SEVERE),
  copied from the source record.
- `AESTDTC` holds the event onset date, copied from the source record.
- `SEV_RANK` is the event's severity rank within its subject: the
  worst severity ranks first, and the later onset date breaks a
  severity tie. Events tied on both share a rank, and the next rank
  is skipped.
- `SEV_SEQ` counts the same ordering without gaps: events tied on
  every ordering rule take consecutive numbers in the order they
  appear in the source data.

**Note:** the ranking runs over each subject's records after every
record's severity is scored MILD = 1, MODERATE = 2, SEVERE = 3, so the
window reads the scored severity together with the stored subject
and onset date; when two records tie on every ordering rule, the
earlier source record comes first.

**Standard:** ADaM | **Domain:** ADAE
