# Collected Medications with Dose Text and Timing References

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-cm-collected-medications.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the CM record for each collected medication from the
concomitant-medication form, carrying the reported name, indication, dose,
frequency, route, and dates as collected, and the form's "taken before
study" and "ongoing" boxes as timing references instead of invented dates.

**Input:** the medication log for two subjects: each medication is one
repeat of the `IG.CM` item group, and the repeat number becomes `CMSEQ`.
The dose is collected as free text, and the frequency and route as the
labels printed on the form (`Once daily`, `Twice daily`, `By mouth`). One
subject reports a single medication with complete dates and a single dose;
the other reports a medication started before the study with only year and
month known and the "ongoing" box checked, plus a medication whose dose was
collected as a range.

**Variables:**

- `CMTRT` is the medication name as reported, kept exactly as written.
- `CMINDC` is the indication as reported.
- `CMDOSE` is the collected dose when it is a plain number, such as `100`;
  empty otherwise.
- `CMDOSTXT` is the collected dose when it is not a plain number, such as
  the range `200-400`; empty when `CMDOSE` carries the dose.
- `CMDOSU` is the unit of the collected dose.
- `CMDOSFRQ` is the collected frequency label mapped to controlled
  terminology: `Once daily` becomes `QD` and `Twice daily` becomes `BID`.
- `CMROUTE` is the collected route label mapped to controlled terminology:
  `By mouth` becomes `ORAL`.
- `CMSTDTC` is the start of the medication as collected, keeping partial
  dates at their collected precision: year and month alone stay `2023-09`.
- `CMENDTC` is the end of the medication as collected; empty when no end
  date was collected.
- `CMSTRTPT` is `BEFORE` when the "taken before study" box is checked, and
  `CMSTTPT` names the reference point, `SCREENING`; both empty otherwise.
- `CMENRTPT` is `ONGOING` when the "ongoing" box is checked, and `CMENTPT`
  names the reference point, `END OF STUDY`; both empty otherwise.

**Note:** a frequency or route label the mappings do not list stops the
run rather than passing through unmapped. An ongoing medication has no end
date, and `CMENRTPT` records why, so an empty `CMENDTC` is never left to
stand for "ongoing" on its own.

**Standard:** SDTM | **Domain:** CM
