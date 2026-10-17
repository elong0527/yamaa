# Reject Rename-Only Intermediate

**Goal:** attach the standardized medication name (`CMDECOD`) to each
collected concomitant medication by joining the coding team's output
to the WHODrug extract.

**Input:** collected medication verbatims (`CMTRT`), a coding-team
table carrying the selected WHODrug record and ATC code per
medication, plus a WHODrug extract carrying the preferred name per
record and ATC code.

**Variables:**

- `CMDECOD` would be the preferred name read from the WHODrug extract
  matched on the coding team's record and ATC code. The coding-team table
  is also given a second name that selects, computes, and reshapes
  nothing, so the run is rejected before any data is read and no artifact
  is accepted.

## How to fix

Drop the alias and read the coding-team table under its own name. Its
record is matched to each row by study, subject, and sequence number, so
also read its sequence number as an integer to compare with `CMSEQ`:

```yaml
input:
  CM_RAW: input/cm_raw.csv
  CODING:
    path: input/coding.csv
    types: {CMSEQ: int}
  WHODRUG: input/whodrug.csv
base: CM_RAW

columns:
  - name: CMDECOD
    type: str
    label: Standardized Medication Name
    derivation: WHODRUG_LOOKUP.PREFERRED_NAME
```

A named intermediate must narrow, derive, or reshape its dataset
([LOOKUP-4](../../rules/lookup.md#lookup-4)).
