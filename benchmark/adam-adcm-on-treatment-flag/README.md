# Flag medications taken during treatment

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adcm-on-treatment-flag.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Lifecycle:** reviewed - has discussion comments or GitHub issues.

**Goal:** flag each concomitant medication (CM) taken during the
treatment period, adding `ONTRTFL`.

**Input:** medication records carrying sequence number (`CMSEQ`),
medication name (`CMTRT`),
start date (`ASTDT`) and end date (`AENDT`), together with the
subject's treatment start (`TRTSDT`) and treatment end (`TRTEDT`)
dates from the subject-level analysis dataset (ADSL). A date that
was never collected stays empty, and a subject without an ADSL
record keeps their medications with both treatment dates empty.

**Variables:**

- `ONTRTFL`: `Y` when the medication dates overlap the treatment
  period; blank otherwise. A medication ending before treatment
  starts, or starting after treatment ends, is left unflagged.

**Note:** a medication with a missing start or end date is
assumed to overlap unless its known dates rule overlap out. A
medication for a subject with no treatment start date is left
unflagged, while a missing treatment end date leaves the
treatment period open-ended.

**Standard:** ADaM | **Domain:** ADCM
