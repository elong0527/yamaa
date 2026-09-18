# Derive AGE from a birth date

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-dm-birth-date-age.html)

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
  keeps its anniversary on February 28 in a common year.
- **AGEU**: `YEARS` whenever **AGE** is present.

**Partial dates:** a year-month or year-only birth date cannot give completed
years, so **AGE** and **AGEU** stay missing while **BRTHDTC** keeps the
truncated text. A missing birth date leaves all three missing.

**Subjects:** 001 was born before the consent-date anniversary (40), 002 after
it (39), 003 reported year and month only, 004 year only, and 005 reported no
birth date at all.

**Standard:** SDTM | **Domain:** DM
