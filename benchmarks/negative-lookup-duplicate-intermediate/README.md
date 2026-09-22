# Reject Repeated End-of-Study Records

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-lookup-duplicate-intermediate.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive each subject's end-of-study date from disposition
records that assert one end-of-study record per subject.

**Input:** demographics rows with disposition records carrying
category (`DSCAT`), decoded term (`DSDECOD`), and start date
(`DSSTDTC`).

**Problem:**

- `DS_EOS` names each subject's end-of-study record from eligible
  disposition events, asserting the subject key is unique across
  them. One subject carries two eligible end-of-study records, so
  the key repeats and the run is rejected before any output row is
  built.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Give each subject exactly one eligible end-of-study record in the
source data. When the source cannot promise that, drop the uniqueness
assertion and choose the record explicitly instead:

```yaml
intermediates:
  - id: DS_EOS
    dataset: DS
    filter: "DS.DSCAT = 'DISPOSITION EVENT'"
    order_by: [DS.DSSTDTC]
    keep: last
```

Keep the uniqueness assertion only for eligible sets that truly
carry each key once.
