# New anti-cancer therapy start

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-new-anticancer-therapy-date.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive the start date of new anti-cancer therapy given during
the study (`NACTDT`), its study day relative to the start of study
treatment (`NACTDY`), and a flag marking subjects who started such
therapy (`NACTFL`).

**Input:** one subject-level table carrying the treatment start date
`TRTSDT`, alongside concomitant medication records carrying `CMCAT` and
`CMSTDTC`, plus procedure records carrying `PRCAT`, `PRSCAT`, and
`PRSTDTC`.

**Variables:**

- `NACTDT` is the earlier of the first qualifying medication start
  (`CMSTDTC` with `CMCAT` equal to `ON TREATMENT`) and the first
  qualifying procedure start (`PRSTDTC` with `PRCAT` equal to `CANCER
  RELATED` and `PRSCAT` equal to `ON TREATMENT`); it is absent when the
  subject started no qualifying therapy during the study. Therapy
  recorded as `PRIOR TREATMENT` does not qualify, nor do procedures
  given for reasons other than the cancer.
- `NACTDY` is the study day of `NACTDT` counted from `TRTSDT`, with the
  treatment start date itself as day 1; it is absent when `NACTDT` is
  absent.
- `NACTFL` holds `Y` when `NACTDT` is present and is absent
  otherwise.

**Note:** a flagged therapy start falls on or after the treatment
start date.

**Standard:** ADaM | **Domain:** ADSL
