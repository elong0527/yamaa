# Reject Parent Escape

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-path-parent-escape.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

This fixture retains the reference engine's historical containment rejection
and locked expected report. Under the approved declaring-file policy (#1905),
absolute and parent-relative local spellings are permitted; an absent file
fails at the declared location. Current public Python/R path behavior is
qualified by independently authored installed tests. This fixture records
reference-only policy history until the coordinated reference transition and
is not evidence that the replacement public routes reject these path forms.

**Goal:** one record for each subject and collected laboratory
parameter carrying `SEX`, `AVAL`, and `ANRHI`.

**Input:** collected laboratory results carrying the collected
result (`LBSTRESN`), test code (`LBTESTCD`), and sex (`SEX`),
together with a reference table of upper limits read from
`../reference/lbref.csv`, a location above the study.

**Variables:**

- `SEX` would be the sex the limits are chosen by, taken from
  `SEX`.
- `AVAL` would be the collected result, taken from `LBSTRESN`.
- `ANRHI` would be the upper limit of normal for that test and
  sex, taken from the reference entry whose test code and sex
  equal `LBTESTCD` and `SEX`; missing when no entry matches or the
  sex is missing.

**Note:** the limits live outside the study, so the run is rejected
before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Bring the governed limits into the study and read them from there:

```yaml
input:
  LBREF:
    path: input/lbref.csv
```

If the limits are maintained centrally, copy the approved version into the
study when it is approved. A study that reaches above itself for data cannot
state which version produced its results.
