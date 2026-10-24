# Refuse a quantile beside a mean

**Goal:** carry each subject's age and add `AGEMED`, the median age of the
subjects in the study.

**Input:** one subject-level record per subject with the collected age.

**Variables:**

- `AGEMED` would be the median of the subjects' ages, the same on every
  record. The request states a quantile of 0.5 but averages the ages, and an
  average reads no quantile, so the run is rejected before any data is read.

## How to fix

Ask for the median itself, which needs no quantile:

```yaml
- name: AGEMED
  type: float
  derivation:
    aggregate:
      group_by: [STUDYID]
      expr: "MEDIAN(AGE)"
```

`QUANTILE(AGE)` with `quantile: 0.5` gives the same value. If the mean age
was meant, remove the quantile and name the column for the mean.
