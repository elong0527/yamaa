# Reference start and end dates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-dm-reference-dates.html)

**Goal:** build one record per enrolled subject carrying informed
consent date (`RFICDTC`), first and last exposure dates (`RFXSTDTC`,
`RFXENDTC`), reference start date (`RFSTDTC`), and reference end
date (`RFENDTC`).

**Input:** collected demographics rows with consent date, exposure
rows with start and end dates and sequence number, disposition rows
with category and start date, and adverse event rows with end date.

**Variables:**

- `RFICDTC` is the consent date as collected; always present.
- `RFXSTDTC` is the earliest non-missing exposure start date;
  missing when no exposure row carries a start date.
- `RFXENDTC` is the end date of the last exposure row among rows
  with a non-missing end date, ordered by end date then sequence
  number; missing when none carries an end date.
- `RFSTDTC` is the reference start date; repeats the first
  exposure date, so missing whenever the first exposure date is
  missing.
- `RFENDTC` is the latest of the last exposure end date, the
  latest disposition start date from rows coded `DISPOSITION EVENT`
  in category, and the latest adverse event end date; missing when
  all three are missing.

**Note:** only rows coded `DISPOSITION EVENT` count toward the
reference end date; exposure, disposition, and adverse event rows
are read within the same subject only, and an enrolled subject with
no exposure has missing exposure and reference start dates.

**Standard:** SDTM | **Domain:** DM
