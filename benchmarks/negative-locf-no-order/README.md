# Reject carry-forward without an assessment order

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-locf-no-order.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

Input: Planned vital-sign assessments with collected values or gaps.

Expected failure: Carrying an earlier value requires an assessment order.

## How to fix

Specify the clinical order in which an earlier assessment is selected:

```yaml
locf:
  source: AVALCOL
  window:
    group_by: [USUBJID, PARAMCD]
    order_by: [AVISITN]
```
