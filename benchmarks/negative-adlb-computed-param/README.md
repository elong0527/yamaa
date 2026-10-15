# Reject Self-Referential Parameter

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-adlb-computed-param.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `ADT`, the analysis date, and `AVAL`, the analysis
value, for each subject, collection date, and parameter: the alanine
aminotransferase (`ALT`) and aspartate aminotransferase (`AST`)
results keep their measured values, while a third ratio parameter
(`ASTALT`, "Aspartate to Alanine Aminotransferase Ratio") divides
the day's aspartate result by its alanine result.

**Input:** each input record is one collected result from LB,
carrying its study, subject, and sequence number alongside
`LBDTC`, the date the sample was collected; `LBTESTCD`, the test
code; `LBTEST`, the test name; and `LBSTRESN`, the numeric result,
which is missing when no numeric value was returned. One subject's
only record is an aspartate result, and one alanine result is zero.

**Variables:**

- `ADT`: the collection date, taken directly from `LBDTC`.
- `AVAL`: the analysis value. On a transaminase record it repeats
  `LBSTRESN`. On the ratio record it divides the day's aspartate
  result by the alanine result. A ratio record exists for each
  alanine record, and its value is missing when that day has no
  aspartate record or either result has no value; a zero alanine
  result likewise leaves the ratio missing.

The ratio reads its two values from other records of the result
being built, but those values are themselves the `AVAL` under
definition, so no value can be assembled. The run is rejected
before any data is read, and no artifact is accepted. The expected
file shows what the completed records would contain, but no row
is produced.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

First decide which run owns the ratio. A parameter whose value comes from
other parameters belongs to a run that reads them as data, so build the two
transaminase parameters first and take the ratio in a second specification
that declares the completed dataset as one of its sources:

```yaml
input:
  ADLB_RAW: adlb.csv
```

The transaminase results are then ordinary records with keys, and the ratio is
an ordinary lookup of the aspartate result on the alanine record's date.

Do not reach the other parameter by counting rows instead. Which row sits one
position away depends on how the records happen to be ordered, so a sample
that answers correctly today stops doing so as soon as a third parameter is
added.
