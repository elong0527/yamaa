# Reject Rename-Only Intermediate

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-lookup-rename-intermediate.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

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

**Standard:** SDTM | **Domain:** CM

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
    derivation:
      lookup:
        dataset: WHODRUG
        key_base: [CODING.DRUG_RECORD_NO, CODING.ATC_CODE]
        key: [DRUG_RECORD_NO, ATC_CODE]
        value: PREFERRED_NAME
```

A named intermediate must narrow, derive, or reshape its dataset
([REQ-1248](../../rules/operations/lookup.md#req-1248)).
