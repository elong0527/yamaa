# Reject carry-forward without an assessment order

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-locf-no-order.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** fill the analysis value (`AVAL`) of each planned vital-sign
visit with the value collected there, or carry forward the latest
earlier collected value for the same subject and parameter.

**Input:** planned vital-sign assessments, one record per subject,
parameter, and planned visit number (`AVISITN`), each holding a
collected value or a gap.

**Variables:**

- `AVAL` would be the value collected at the visit when there is one,
  otherwise the latest earlier collected value for the same subject
  and parameter, and blank when no earlier value exists. Zero is a
  collected value, not a gap.

The input covers the cases that matter: a leading gap at visit 0, a
trailing gap after the last collected value, a collected zero that
must not be confused with a gap, and a parameter collected nowhere.
But carrying a value forward needs an order that says which visits
count as earlier, and the window names none, so the run is rejected
before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADVS

## How to fix

Name the assessment order in the window, here the planned visit
number:

```yaml
locf:
  source: AVALCOL
  window:
    group_by: [USUBJID, PARAMCD]
    order_by: [AVISITN]
```
