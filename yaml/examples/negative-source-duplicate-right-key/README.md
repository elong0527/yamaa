# Reject an actual treatment with two source records for one subject

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-source-duplicate-right-key.html)

**Goal:** build one record per subject carrying the actual
treatment for period 01 (`TRT01A`).

**Input:** demographics records plus analysis-subject records
carrying a treatment (`TRT01A`).

**Variables:**

- `TRT01A` would contain the treatment from the analysis-subject
  record for the same subject.

When more than one analysis-subject record shares one subject's
identifiers, nothing says which treatment answers. Taking either
one would report a treatment the study data does not single out,
so the run is rejected with no artifact accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Reconcile the analysis-subject source so each subject has one supported
treatment. If multiple records are legitimate, add a field that expresses the
choice, such as an effective timestamp, and select by it explicitly:

```yaml
source:
  variable: ADSL_RAW.TRT01A
  multiple_matches:
    order_by: [ADSL_RAW.EFFECTIVEDTC]
    keep: last
```

Do not use file order or treatment text as a substitute for a study rule.
