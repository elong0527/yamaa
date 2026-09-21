# Reject Per-Record RDI

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-dose-intensity.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive cumulative dose (`DOSECUM`) and relative dose
intensity (`RDI`) by subject and treatment.

**Input:** a subject-treatment inventory carrying the dose unit
(`EXDOSU`), with component exposure records carrying the
treatment (`EXTRT`), the administered dose (`EXDOSE`), and the
planned dose (`EXPLDOS`) on each administration record.

**Variables:**

- `EXTRT` names the treatment component, and `EXDOSU` is the unit
  that component is measured in.
- `DOSECUM` is the total administered dose across the
  component's exposure records.
- `RDI` is meant to be that total as a percentage of the planned
  dose, but would divide the summed actual doses by the planned
  dose of one exposure record, and no row is produced because the
  run is rejected before any data is read: the planned dose is
  recorded on each administration record rather than once for
  the treatment, so which record supplies it would decide the
  answer.

**Standard:** ADaM | **Domain:** ADEX

## How to fix

First decide what the denominator means. If each exposure row carries its own
planned administration, total both actual and planned dose:

```yaml
derivation:
  aggregate:
    expr: >-
      100 * SUM(EX.EXDOSE) / NULLIF(SUM(EX.EXPLDOS), 0)
```

If the denominator is one treatment-level plan, store it once in the subject
treatment source, bind it to a numeric column, and compute `RDI` from that
column and `DOSECUM`. Do not select one exposure row arbitrarily.
