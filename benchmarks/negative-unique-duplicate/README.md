# Reject Duplicate SITEID

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-unique-duplicate.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the analysis dataset with one record per subject
carrying `SITEID`.

**Input:** collected demographics records with `SITEID` giving the
enrolling site.

**Variables:**

- `SITEID` would hold the site the subject enrolled at, copied from
  the collected site.

**Note:** the completed dataset is checked for one record per
site, but a site that enrolled more than one subject repeats its
`SITEID`, which then cannot identify a subject record. The run is
rejected and no artifact is accepted.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Decide whether a record represents a subject or a site before editing
anything. Assert uniqueness on the columns that truly identify a record.
Either check the subject identifiers:

```yaml
verifications:
  - unique:
      columns: [STUDYID, USUBJID]
```

or, when one record per site is the intent, aggregate the subjects to site
records before asserting.
