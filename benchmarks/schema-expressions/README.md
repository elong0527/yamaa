# Expression Forms

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-expressions.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** pin how each column's value is written and read: fixed
values, values copied by naming their source column, age bands, a
population flag chosen by a condition, first-available country
selection, joining text, and the largest and smallest of several
collected weights.

**Input:** one `spec.yaml`. `DM` carries one record per subject: sex,
age, first-dose date, country, site country, and two collected
weights, any of which may be blank.

**Columns:**

- `STUDYID` holds a fixed study code written directly in the column.
- `USUBJID` and `SEX` copy the collected values by naming the source
  column alone.
- `AGEGRP` places each age in a band: under 18, 18 to 64, or 65 and
  over. Each band includes its lower edge, so an age of exactly 65 is
  in the oldest band; a subject with no age carries the fallback text
  `NOT REPORTED`.
- `SAFFL` flags the safety population: `Y` for a subject with a
  first-dose date, `N` for a subject without one.
- `COUNTRY` takes the subject's own country, or the site country when
  the subject's own entry is blank. When both are blank it carries the
  declared fallback text.
- `SUBJLBL` joins the fixed word `Subject` to the subject number.
- `WTMAX` carries the larger of the two collected weights, skipping a
  blank one; with both blank it stays blank.
- `WTMIN` carries the smaller of the two collected weights, skipping a
  blank one; with both blank it stays blank.

**Standard:** ADaM | **Domain:** ADSL
