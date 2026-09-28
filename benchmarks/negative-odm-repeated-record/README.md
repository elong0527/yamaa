# Reject a Record the Extract Repeats

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-odm-repeated-record.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `SEX` for one record per subject from the subject's
collected answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item, exported with an extra field naming the user who entered
each record. The export repeats one subject's sex record, once under each
of two users.

**Variables:**

- `SEX` would be the subject's answer to the sex question.

The two records agree on the subject, visit, form, repeat, and item, and
differ only in the user field, which is the vendor's own and identifies no
record. The run stops and reports that the input repeats one record.

**Standard:** SDTM | **Domain:** DM

## How to fix

A repeated record is a defect in the extract, not a choice the mapping can
make. Remove the duplicate from the export, or have the vendor deliver the
current record only, and run the same mapping again.
