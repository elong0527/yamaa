# Reject a Missing Output Key

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-output-missing-key.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry an analysis date (`ADT`), study day (`ADY`), and measured
value (`AVAL`) into analysis visits (`AVISIT`).

**Input:** pre-derived vital signs records, one per measurement, with a
study day that can be missing.

**Variables:**

- `AVISIT` would be `SCREENING` before day 0, `BASELINE` on days 0 and 1,
  `WEEK 2` on days 2 to 21, `WEEK 4` on days 22 to 42, and
  `POST-TREATMENT` from day 43. It is blank when the study day is missing.

**Note:** analysis visit is part of each record's identity. A record with
no study day has no analysis visit, so the completed dataset fails its
identity check and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADVS

## How to fix

Decide whether the record belongs in an analysis visit. If a governed
source supplies the missing study day, correct that source. Otherwise,
exclude the record before constructing the output rows:

```yaml
rows:
  - id: windowed
    filter: "ADVS_RAW.ADY IS NOT NULL"
    derivations: {}
```

If the dataset must retain measurements without a visit, identify rows
by the collected record instead, for example with `VSSEQ` in `keys`.
