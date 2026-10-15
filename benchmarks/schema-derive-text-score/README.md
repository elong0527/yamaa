# Derive Text Score

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-derive-text-score.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** demonstrate the per-record `derive` step of an aggregation:
convert text questionnaire responses to numbers, then sum them.

**Input:** questionnaire responses with text results (`QSORRES`)
for the physical functioning scale. Each subject and visit with a
record for the first item (`PF01`) gets one score record.

**Variables:**

- `PARAMCD`: `PFSCORE` on the score record.
- `PARAM`: the subscale name.
- `AVAL`: the sum of the visit's physical functioning responses, each
  read as a number. A blank response is left out of the sum; a
  response that is not a number stops the run.

**Note:** there is no separate text-to-number step. Declaring each
record's response as a decimal number is what converts it, one record
at a time, before the sum runs.

**Standard:** ADaM | **Domain:** ADQS
