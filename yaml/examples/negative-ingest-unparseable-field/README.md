# Reject a dose recorded with its unit

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-ingest-unparseable-field.html)

**Goal:** build one record per subject and treatment carrying
`DOSECUM`, the total dose administered across the matching
exposure records.

**Input:** a subject-treatment inventory carrying `EXTRT`, plus
component exposure records carrying `EXTRT`, `EXSEQ`, and
`EXDOSE`. One exposure record carries `200 mg` where the declared
type is numeric.

**Variables:**

- `DOSECUM` would be the total of `EXDOSE` across the matching
  exposure records for the same subject and treatment.

The run is rejected while reading the stored `200 mg` value
against its declared numeric type, so no artifact is accepted.

**Standard:** ADaM | **Domain:** ADEX

## How to fix

Normalize the source into separate value and unit fields before declaring the
dose numeric. For this record, the governed input should carry `EXDOSE` as
`200` and `EXDOSU` as `mg`; then the existing declaration is valid:

```yaml
EX:
  path: input/ex.csv
  types:
    EXDOSE: float
```

If the composite text must remain unchanged, ingest it as text, preserve it
for traceability, and derive a validated numeric dose before attempting the
aggregate. A numeric type declaration must not strip the unit implicitly.
