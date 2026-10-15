# Age From Birthdate

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-age.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `AGE` in completed years at the sponsor's reference date
(informed consent, `RFICDTC`) from the collected birth date, keeping
`BRTHDTC` at its collected precision.

**Input:** electronic data capture (EDC) output in long form, one row per
collected item; each subject carries a birth date item (`IT.DM.BRTHDT`,
as collected) and an informed consent date item (`IT.DM.RFICDTC`).

**Variables:**

- `BRTHDTC` is the birth date exactly as collected, as ISO 8601 text cut
  off at the collected precision: `1985-03-10` (day), `1985-03` (month),
  or `1985` (year).
- `AGE` is the whole calendar years between the birth date and the
  informed consent date: one year fewer when the birthday falls after the
  consent date's month and day. A February 29 birthday has its
  anniversary on February 28 in a common year. A year-month or year-only
  birth date cannot give completed years, so `AGE` stays missing.
- `AGEU` is `YEARS` whenever `AGE` is present, missing otherwise.

**Note:** a missing birth date leaves `BRTHDTC`, `AGE`, and `AGEU`
missing.

**Standard:** SDTM | **Domain:** DM
