# Age From Birthdate

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-dm-age.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive **AGE** in completed years at the sponsor's reference date
(informed consent, **RFICDTC**) from the collected birth date, keeping
**BRTHDTC** at its collected precision.

**Input:** EDC output in long form, one row per collected item; subjects
carry **BRTHDT** (birth date as collected) and **RFICDTC** rows.

**Variables:**

- **BRTHDTC**: birth date exactly as collected. ISO 8601 right truncation
  keeps the precision: `1985-03-10` (day), `1985-03` (month), `1985` (year).
- **AGE**: whole calendar years between the birth date and the informed
  consent date, using anniversary arithmetic: the birthday falling after the
  reference date's month and day subtracts one year. A February 29 birthday
  keeps its anniversary on February 28 in a common year. A year-month or
  year-only birth date cannot give completed years, so **AGE** stays missing.
- **AGEU**: `YEARS` whenever **AGE** is present, missing otherwise.

**Note:** a missing birth date leaves all three variables missing.

**Standard:** SDTM | **Domain:** DM
