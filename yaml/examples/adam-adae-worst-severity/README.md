# Flag the worst-severity event per preferred term

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-worst-severity.html)

**Goal:** rank severity and flag the worst-severity
treatment-emergent event for each subject and preferred term
(`AEDECOD`), adding `AESEVN` and `AWSEVFL`.

**Input:** adverse event (AE) records carrying dictionary-derived
term (`AEDECOD`), analysis start date (`ASTDT`), severity (`AESEV`),
and treatment-emergent flag (`TRTEMFL`).

**Variables:**

- `AESEVN`: numeric rank of `AESEV`, `1` for MILD through `3` for
  SEVERE; blank when no severity was collected.
- `AWSEVFL`: `Y` on the eligible event with the greatest `AESEVN`
  for the subject and preferred term (`AEDECOD`); blank otherwise.
  Ties break by earliest `ASTDT`, then lowest `AESEQ`, so exactly
  one event per term is flagged.

**Note:** only a treatment-emergent event with a graded severity is
eligible, so a preferred term (`AEDECOD`) whose events are all
ineligible has no flagged event.

**Standard:** ADaM | **Domain:** ADAE
