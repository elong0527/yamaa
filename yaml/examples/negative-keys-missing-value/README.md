# Reject a record that no analysis visit identifies

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-keys-missing-value.html)

**Goal:** carry the analysis date (`ADT`), study day (`ADY`), and
measured value (`AVAL`) through from a pre-derived ADVS slice
(`ADVSPRE`), placing each record in the analysis window its study
day falls in.

**Input:** pre-derived ADVS slice carrying analysis date (`ADT`),
study day (`ADY`), and measured value (`AVAL`).

**Variables:**

- `ADT` would be the record's analysis date, carried through from
  the pre-derived slice as given, and missing when the slice date
  is missing.
- `ADY` would be the record's study day, carried through from the
  pre-derived slice as given, and missing when the slice day is
  missing.
- `AVAL` would be the value measured, carried through from the
  pre-derived slice as given.
- `AVISIT` would be the analysis window the study day falls in,
  and blank when the study day is missing so the record belongs
  to no window.

A record is identified here by study, subject, parameter, and
analysis window. A record left with a blank window carries no
identity. The values are complete before that is checked, and the
expected file records the completed rows presented to the check,
but the run still fails and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADVS

## How to fix

Decide which grain the dataset is on. If it is one record per analysis visit,
every record must fall in a window. Recover the analysis date in the governed
source where it is available. Where it is not, keep the record out of the
dataset rather than give it a place it does not have. A row template selects
the records that become rows, so move one column's derivation into it and
leave the rest where they are:

```yaml
- name: AVAL
  type: float

rows:
  - id: windowed
    filter: "ADVSPRE.ADY IS NOT NULL"
    derivations:
      AVAL:
        source: ADVSPRE.AVAL
```

If it is one record per collected record, identify rows by the collected
record instead, and a record with no window keeps an empty one:

```yaml
keys: [STUDYID, USUBJID, PARAMCD, VSSEQ]
```

Do not label the record into a placeholder window such as `NOT ASSIGNED`. That
keeps the row by asserting an analysis visit the data does not support, and a
second such record collides with the first.
