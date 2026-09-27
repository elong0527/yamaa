# Flag the End-of-Treatment Laboratory Record

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adlb-end-of-treatment.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** mark each subject's end-of-treatment record for every laboratory
test (`EOTFL`), carrying the protocol endpoint qualifier it rests on
(`ENDPOINT`).

**Input:** laboratory (LB) records carry a numeric sequence number, a test
code (`LBTESTCD`), a visit number (`VISITNUM`), and a numeric result.
Supplemental laboratory
qualifier (SUPPLB) records carry the same sequence as eight-character,
space-padded text, one qualifier name and value per record; only the
protocol endpoint qualifier is read.

**Variables:**

- `AVAL` is the numeric laboratory result.
- `ENDPOINT` is `Y` when a supplemental endpoint qualifier with the same
  study, subject, and sequence number marks the record; empty otherwise.
- `EOTFL` is `Y` on one record per subject and test: the endpoint-marked
  record when there is one, otherwise the record from the latest visit.
  Every other record is empty.

**Note:** the endpoint qualifier decides the choice even when a later visit
exists, so the flag can sit before the subject's last visit. A record wins
the flag even when its result is missing, and a group with a single record
still gets the flag. Other
qualifiers on the same record, and endpoint qualifiers for records the
laboratory data does not hold, change nothing.

**Standard:** ADaM | **Domain:** ADLB
