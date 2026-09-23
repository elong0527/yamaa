# Worst Severity Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-worst-severity.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** rank severity and flag the worst-severity
treatment-emergent event for each subject and preferred term
(`AEDECOD`), adding `AESEVN` and `AWSEVFL`.

**Input:** adverse event (AE) records carrying body system or organ
class (`AEBODSYS`), dictionary-derived term (`AEDECOD`), analysis
start date (`ASTDT`), severity (`AESEV`), and treatment-emergent
flag (`TRTEMFL`).

**Variables:**

- `AESEVN`: numeric rank of `AESEV`, `1` for MILD through `3` for
  SEVERE; blank when no severity was collected.
- `AWSEVFL`: `Y` on the eligible event with the greatest `AESEVN`
  for the subject and preferred term (`AEDECOD`); blank otherwise.
  Ties break by earliest `ASTDT`, then lowest `AESEQ`, so exactly
  one event per term is flagged.

**Note:** only a treatment-emergent event with a graded severity is
eligible, so a preferred term (`AEDECOD`) whose events are all
ineligible has no flagged event. The data exercises both tie-breaks
(a same-day, same-severity tie broken by lowest `AESEQ`, and a
same-severity tie broken by earliest `ASTDT`) as well as a
multi-record term with no eligible event.

**Standard:** ADaM | **Domain:** ADAE
