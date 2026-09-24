# Reject Missing Key

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-keys-missing.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the analysis date (`ADT`), study day (`ADY`), and
measured value (`AVAL`) through from a pre-derived ADVS slice
(`ADVS_RAW`), placing each record in the analysis window its study
day falls in.

**Input:** pre-derived ADVS slice carrying analysis date (`ADT`),
study day (`ADY`), and measured value (`AVAL`).

**Variables:**

- `AVISIT` would be the analysis window the study day falls in:
  `SCREENING` before day 0, `BASELINE` on days 0 and 1, `WEEK 2` on
  days 2 to 21, `WEEK 4` on days 22 to 42, and `POST-TREATMENT` from
  day 43; blank when the study day is missing, so the record belongs
  to no window.

**Note:** a record is identified here by study, subject, parameter,
and analysis window. With nothing else choosing the rows, those
identities are the rows, so they are settled before any other value
is computed. The window is placed from the study day, which is one of
those other values, so the run is rejected before any data is read
and no artifact is accepted. Even if the window read the slice's
study day directly, a record with no study day would have no window
and so no identity. The expected file shows the intended rows, not
an accepted result.

**Standard:** ADaM | **Domain:** ADVS

## How to fix

Decide which keys the dataset is on. If it is one record per analysis visit,
every record must fall in a window. Recover the analysis date in the governed
source where it is available. Where it is not, keep the record out of the
dataset rather than give it a place it does not have. A row template selects
the records that become rows, and each row then comes from one kept record, so
its window can be placed from its study day. Move one column's derivation into
it and leave the rest where they are:

```yaml
- name: AVAL
  type: float

rows:
  - id: windowed
    filter: "ADVS_RAW.ADY IS NOT NULL"
    derivations:
      AVAL:
        source: ADVS_RAW.AVAL
```

If it is one record per collected record, identify rows by the collected
record instead, and a record with no window keeps an empty one:

```yaml
keys: [STUDYID, USUBJID, PARAMCD, VSSEQ]
```

Do not label the record into a placeholder window such as `NOT ASSIGNED`. That
keeps the row by asserting an analysis visit the data does not support, and a
second such record collides with the first.
