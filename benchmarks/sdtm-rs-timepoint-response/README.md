# Timepoint Response

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-rs-timepoint-response.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one RS record per subject and scheduled tumor
assessment holding the visit order (`AVISITN`), the assessment date
(`ADT`), a per-subject record number (`RSSEQ`), the response test
(`RSTESTCD`/`RSTEST`), the response (`RSSTRESC`), and the completion
status (`RSSTAT`).

**Input:** scheduled tumor assessments with visit order and date,
tumor measurement records with lesion group, test code, numeric
result, and completion status, plus the lesion inventory chosen at
study entry with lesion group and lesion identifier.

**Variables:**

- `AVISITN` is the numeric order of the assessment, carried from the
  scheduled assessment; it orders the assessments of a subject.
- `ADT` is the date of the assessment, carried from the scheduled
  assessment.
- `RSSEQ` numbers a subject's response records in visit order,
  starting at 1.
- `RSTESTCD` is `TRGRESP` and `RSTEST` is `Timepoint Response` on
  every record.
- `RSSTRESC` is the response at the assessment: `NE` (not evaluable)
  at baseline, when fewer target lesions were measured than were
  chosen, or when the subject has non-target disease only; `PR`
  (partial response) when the target sum shrank at least 30% from
  baseline; `PD` (progressive disease) when it grew at least 20% from
  baseline; `SD` (stable disease) otherwise. It stays blank when the
  assessment was not done.
- `RSSTAT` is `NOT DONE` for a scheduled assessment with no tumor
  measurement records at all; blank otherwise.

**Note:** the baseline assessment is never compared against itself,
so its response is always `NE`. An assessment missing a lesion
measurement still keeps the records it measured; the incomplete set
makes the response `NE` rather than guessing from partial data.

**Standard:** SDTM | **Domain:** RS
