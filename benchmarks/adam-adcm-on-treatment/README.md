# On-Treatment Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adcm-on-treatment.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** flag each concomitant medication taken while the subject is on
treatment, adding `ONTRTFL`.

**Input:** the subject's medication records (`input/cm.csv`) with the
collected start and end dates, plus the subject-level records
(`input/adsl.csv`) carrying each subject's first and last treatment dates.
A subject without a subject-level record keeps their medications with both
treatment dates empty.

**Variables:**

- `STUDYID`, `USUBJID`, `CMSEQ` are carried through as collected.
- `CMTRT` is the reported medication name, carried through as collected.
- `ASTDT` and `AENDT` are the medication's analysis start and end dates.
- `TRTSDT` and `TRTEDT` are the subject's first and last treatment dates.
- `ONTRTFL` is `Y` when the medication dates overlap the treatment period;
  blank otherwise. A medication ending before treatment starts, or starting
  after treatment ends, is left unflagged.

**Note:** a medication with a missing start or end date is assumed to
overlap unless its known dates rule overlap out. A missing treatment end
date leaves the period open-ended, while a subject with no treatment start
date is never flagged.

**Standard:** ADaM | **Domain:** ADCM
