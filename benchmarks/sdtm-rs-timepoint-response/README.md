# Timepoint Response

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-rs-timepoint-response.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one RS record per subject and scheduled tumor
assessment holding the visit order (`AVISITN`), the assessment date
(`ADT`), a per-subject record number (`RSSEQ`), the response test
(`RSTESTCD`/`RSTEST`), the response (`RSSTRESC`), and the completion
status (`RSSTAT`).

**Input:** long-form Operational Data Model (ODM) item data. Each
scheduled tumor assessment (`IG.VISIT`) carries its visit name, visit
order, and date. Each lesion measured at an assessment is a repeated
item group, `IG.TRTARGET` for a target lesion and `IG.TRNT` for a
non-target one, carrying the lesion link and the longest diameter. The
lesions chosen at study entry are listed in `IG.TUTARGET` and
`IG.TUNT`.

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
  chosen, when the subject has non-target disease only, or when no
  percent change can be computed (for example a zero or missing
  baseline sum); `CR` (complete response) when every target lesion
  has disappeared; `PR` (partial response) when the target sum shrank
  at least 30% from baseline; `PD` (progressive disease) when it grew
  at least 20% from baseline; `SD` (stable disease) otherwise. It
  stays blank when the assessment was not done.
- `RSSTAT` is `NOT DONE` for a scheduled assessment with no tumor
  measurement records at all; blank otherwise.

**Note:** a baseline assessment is never compared against itself, so
its response is `NE` whenever it was done. An assessment with some but
not all chosen target lesions measured is still done, and its response
is `NE` rather than a guess from partial data.

**Standard:** SDTM | **Domain:** RS
