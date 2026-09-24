# Expression Forms

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-expressions.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** pin how each column's value is written and read: fixed
values, copied values, age bands, first-available country selection,
joining text, and the largest and smallest of several collected
weights.

**Input:** `DM` carries four subjects with sex, age, country, site
country, and two collected weights, including a missing age, a
missing country, and a subject with no weight at all.

**Columns:**

- `STUDYID` holds a fixed study code written directly in the column.
- `USUBJID` copies the subject identifier.
- `SEX` copies the collected sex.
- `AGEGRP` places each age in one band: under 18, 18 to 64, or 65 and
  over. The 70-year-old falls in the top band; the subject with no age
  takes the fallback text.
- `COUNTRY` takes the subject's own country, or the site country when
  the subject's own entry is blank. When both are blank it carries the
  declared fallback text.
- `SUBJLBL` joins the fixed word `Subject` to the subject number.
- `WTMAX` carries the larger of the two collected weights, skipping a
  blank one; with both blank it stays blank.
- `WTMIN` carries the smaller of the two collected weights, skipping a
  blank one; with both blank it stays blank.

**Standard:** CDISC | **Domain:** ADSL
