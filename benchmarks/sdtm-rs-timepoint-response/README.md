# Timepoint Response

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-rs-timepoint-response.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one RS record per subject and scheduled tumor
assessment holding the visit order (`AVISITN`), the assessment date
(`ADT`), a per-subject record number (`RSSEQ`), the response test
(`RSTESTCD`/`RSTEST`), the response (`RSSTRESC`), and the completion
status (`RSSTAT`).

**Input:** a single ODM extract (`input/odm.csv`) in long-form item
data: scheduled tumor assessments as `IG.VISIT` (`IT.VISIT.AVISIT`,
`IT.VISIT.AVISITN`, `IT.VISIT.ADT`), tumor measurements as repeated
item groups -- `IG.TRTARGET` for target lesions and `IG.TRNT` for
non-target lesions -- each carrying the lesion link (`IT.TR.TRLNKID`),
the longest diameter (`IT.TR.LDIAM`), and the completion status
(`IT.TR.TRSTAT`), plus the lesion inventory chosen at study entry
(`IG.TUTARGET` / `IG.TUNT` carrying `IT.TU.TULNKID`). Target versus
non-target is carried by the item-group section, so the same diameter
item serves both; no demographics are needed.

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

**Note:** the baseline assessment is never compared against itself,
so its response is always `NE`. An assessment missing a lesion
measurement still keeps the records it measured; the incomplete set
makes the response `NE` rather than guessing from partial data.

**Standard:** SDTM | **Domain:** RS
