# Dataset-Level Related Records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-relrec-dataset-level.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** emit related-records rows for whole-dataset
relationships alongside record-level rows, carrying `IDVAR`,
`IDVARVAL`, and `RELID`.

**Input:** collected tumor identification (TU) and tumor results
(TR) records each carrying a link identifier, plus an adverse
event (AE) and a concomitant medication (CM) sharing one link
identifier.

**Variables:**

- `USUBJID` is empty on a dataset-level row, which relates whole
  domains rather than subject records; it names the subject on a
  record-level row.
- `IDVAR` names the link variable on a dataset-level row
  (`TULNKID`, `TRLNKID`) and the sequence variable on a
  record-level row (`AESEQ`, `CMSEQ`).
- `IDVARVAL` is empty on a dataset-level row and carries the
  sequence number of the related record as text on a
  record-level row.
- `RELTYPE` is blank throughout: no row here needs a relationship
  type beyond what `RELID` already groups.
- `RELID` names the relationship the row takes part in; rows
  sharing a value are related to one another, and every row
  carries one.

**Note:** rows sharing a `RELID` are related to one another
whether they point at whole datasets or single records; a
dataset-level row never names a subject or a record value.

**Standard:** SDTM | **Domain:** RELREC
