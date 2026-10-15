# Dataset-Level Related Records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-relrec-dataset-level.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** emit related-records rows for whole-dataset
relationships alongside record-level rows, carrying `IDVAR`,
`IDVARVAL`, and `RELID`.

**Input:** collected tumor identification (TU) and tumor results
(TR) records each carrying a link identifier, plus adverse event
(AE) and concomitant medication (CM) records, where an AE and a CM
record sharing a link identifier are related. An AE or CM record
without a link identifier gets no row.

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
- `RELTYPE` carries `ONE` on the TU row and `MANY` on the TR row:
  one tumor identification relates to many tumor results. This is
  the only use of `RELTYPE` in SDTM; the record-level AE/CM rows
  leave it blank.
- `RELID` names the relationship the row takes part in; rows
  sharing a value are related to one another, and every row
  carries one. The dataset-level rows share `1`; each
  record-level row carries its record's link identifier, so
  records with different link identifiers land in distinct
  relationships instead of folding into one.

**Note:** rows sharing a `RELID` are related to one another
whether they point at whole datasets or single records; a
dataset-level row never names a subject or a record value.

**Standard:** SDTM | **Domain:** RELREC
