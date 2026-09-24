# Collect medications with dates, ongoing flag, and dose ranges

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-cm-collected-medications.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the CM record for each collected medication from the
concomitant-medication form, carrying the reported name, the dose as
collected, and the start and end dates into the output.

**Input:** the medication log for two subjects: each medication is one
repeat of the `IG.CM` item group, and the repeat number becomes `CMSEQ`.
One subject reports a single medication with complete dates and a
single dose; the other reports a medication started before the study
with only year and month known and the "ongoing" box checked, plus a
medication whose dose was collected as a range.

**Variables:**

- `CMTRT` is the medication name as reported, kept exactly as
  written.
- `CMDOSE` is the amount taken as collected, kept in text form: a
  single amount such as `100`, or a range such as `200-400` when the
  form collected one.
- `CMDOSU` is the unit of the collected amount.
- `CMSTDTC` is the start of the medication as collected, keeping
  partial dates at their collected precision: year and month alone
  stay `2023-09`.
- `CMENDTC` is the end of the medication as collected. A medication
  whose form marks it ongoing has no end date in the output.

**Note:** the form's "ongoing" check has no field of its own in the
output; its meaning is carried by the blank `CMENDTC`.

Standard: SDTM | Domain: CM
