# Reject a parameter computed from the dataset being built

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adlb-computed-parameter.html)

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
which is missing when no numeric value was returned.

**Variables:**

- `ADT`: the collection date, taken directly from `LBDTC`.
- `AVAL`: the analysis value. On a transaminase record it repeats
  `LBSTRESN`. On the ratio record it divides the day's aspartate
  result by the alanine result, and is missing when either side
  has no record or no value that day.

The ratio reads its two values from other records of the result
being built, but those values are themselves the `AVAL` under
definition, so no value can be assembled. The run is rejected
before any data is read, and no artifact is accepted. The expected
file shows what the completed records would contain, but no row
is produced.

**Note:** an alanine result of zero would likewise leave the ratio
missing, since division by zero is not defined.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

First decide which run owns the ratio. A parameter whose value comes from
other parameters belongs to a run that reads them as data, so build the two
transaminase parameters first and take the ratio in a second specification
that declares the completed dataset as one of its sources:

```yaml
datasets:
  ADLBIN: adlb.csv
```

The transaminase results are then ordinary records with keys, and the ratio is
an ordinary lookup of the aspartate result on the alanine record's date.

Do not reach the other parameter by counting rows instead. Which row sits one
position away depends on how the records happen to be ordered, so a sample
that answers correctly today stops doing so as soon as a third parameter is
added.
