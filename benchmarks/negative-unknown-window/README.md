# Reject an undefined visit ordering

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-unknown-window.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** number each subject's vital-sign visits in analysis visit order.

**Input:** two planned visits for one subject, identified by `USUBJID` and
analysis visit number `AVISITN`.

**Variables:** `VISITSEQ` would number the subject's visits from one in
analysis visit order. The requested ordering is undefined, so the run is
rejected before data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADVS

## How to fix

Correct the misspelled reference to the complete declared window:

```yaml
row_number:
  window: VISIT_ORDER
```

If a different ordering is intended, declare it as its own complete window
or write its settings inline.
