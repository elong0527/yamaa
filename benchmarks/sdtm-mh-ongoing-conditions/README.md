# Ongoing Conditions with Partial Dates

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-mh-ongoing-conditions.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** build the medical history (MH) records from the history form,
keeping each condition's start at the precision it was recorded and showing
whether it ended before, or was still going at, the screening visit.

**Input:** one record per condition reported on the form, each numbered by
its record sequence (`MHSEQ`), with the collected start year and month,
the collected end date, the ongoing tick box, and the screening visit
date recorded on the same form.

**Variables:**

- `MHTERM` is the condition as written on the form.
- `MHSTDTC` is the start of the condition at the precision recorded: the
  year alone (for example `2015`) when only the year was known, or year and
  month (for example `2019-03`); empty when nothing was recorded.
- `MHENDTC` is the date the condition ended, when an end was recorded.
- `MHENRTPT` says how the condition's end relates to the screening visit:
  `ONGOING` when the tick box said it was still active, `BEFORE` when the
  recorded end date comes before the screening visit date recorded on the
  form; empty otherwise.
- `MHENTPT` names the visit the end reference is measured against,
  `SCREENING`; empty when there is no end reference.

**Note:** a condition still active at the screening visit carries no end
date: the ongoing answer is the record of its end status, and the
screening visit is named as the reference for both `ONGOING` and `BEFORE`.

**Standard:** SDTM | **Domain:** MH
