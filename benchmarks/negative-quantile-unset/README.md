# Refuse a quantile without its proportion

**Goal:** carry each subject's baseline weight and add `WGTP90`, the 90th
percentile of the baseline weights of the subjects in the study.

**Input:** one subject-level record per subject with the baseline weight in
kilograms.

**Variables:**

- `WGTP90` would be the 90th percentile of the subjects' baseline weights,
  the same on every record. The request asks for a quantile without saying
  at which proportion of the sorted weights, and no proportion is assumed, so
  the run is rejected before any data is read.

## How to fix

State the proportion beside the quantile: 0.9 for the 90th percentile.

```yaml
- name: WGTP90
  type: float
  derivation:
    aggregate:
      group_by: [STUDYID]
      quantile: 0.9
      expr: "QUANTILE(WEIGHTBL)"
```
