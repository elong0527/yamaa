# Reject Unknown Field

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-types-unknown-field.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build one record per subject and treatment (`EXTRT`),
carrying the total dose (`DOSECUM`) across the exposure records.

**Input:** subject-treatment records carrying treatment (`EXTRT`),
and exposure records carrying treatment (`EXTRT`), sequence number
(`EXSEQ`), and dose (`EXDOSE`).

**Variables:**

- `EXTRT` would be the treatment name, carried over from the
  subject-treatment records.
- `DOSECUM` would be the total dose across the subject's exposure
  records for that treatment.

**Note:** the list of exposure fields to be read as numbers also
names a field the exposure file does not carry. Guessing which
field was meant would total the wrong values, so the run is
rejected before any data is read and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADEX

## How to fix

Confirm which exposure column holds the dose. Here it is `EXDOSE`, which is
already declared numeric, so remove the entry for `EXDOSEN`, which names no
column of `input/ex.csv`:

```yaml
input:
  TRT: input/subject_treatment.csv
  EX:
    path: input/ex.csv
    types:
      EXDOSE: float
```

Keep the `EXDOSE` entry: without it the dose is read as text and cannot be
totalled.
