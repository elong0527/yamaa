# Reject an Answer Recorded at Two Visits

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-odm-not-unique.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `SEX` for one record per subject from the subject's
collected answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item. One subject is asked the sex question at screening and
again at baseline, and gives the same answer both times.

**Variables:**

- `SEX` would be the subject's answer to the sex question.

That subject's question has two records, one per visit. Two records are not
one answer even when they agree, so the run stops rather than choosing one,
and it names the visit as the field that tells them apart.

**Standard:** SDTM | **Domain:** DM

## How to fix

Decide which visit's answer is the subject's sex, and read that one:

```yaml
derivation:
  odm: {item: ODM.IT.DM.SEX, event: SCREENING}
```

Reading the screening answer gives one record for every subject, whether or
not the question was asked again.
