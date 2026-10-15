# Reject Unit Suffix

**Goal:** build one record per subject and treatment carrying
`DOSECUM`, the total dose administered across the matching
exposure records.

**Input:** a subject-treatment inventory carrying `EXTRT`, plus
component exposure records carrying `EXTRT`, `EXSEQ`, and
`EXDOSE`, where the dose is declared numeric.

**Variables:**

- `DOSECUM` would be the total of `EXDOSE` across the matching
  exposure records for the same subject and treatment, and
  missing when no matching record carries a dose.

A stored dose that is not a plain number, such as `200 mg` with
its unit attached, is rejected while the exposure records are
read, so no artifact is accepted.

## How to fix

Normalize the source into separate value and unit fields before declaring the
dose numeric. For this record, the corrected input should carry `EXDOSE` as
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
