# Create DM from EDC extract

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-dm-basic.html) [![Lifecycle: finalized](https://img.shields.io/badge/Lifecycle-finalized-brightgreen)](https://github.com/elong0527/yamaa/blob/main/yaml/examples/README.md#lifecycle)

**Lifecycle:** finalized - a human decided to finalize this example.

**Goal:** build one Demographics (DM) record per subject: sex (SEX), age
(AGE), planned arm (ARM), actual arm (ACTARM) and reason not assigned
(ARMNRS).

**Input:** EDC output in long form, one row per
collected item; e.g. subject 001 has **SEX**, **AGE** and **ARM** rows.

**Variables:**

- **SEX**: recorded sex coded `M` (Male), `F` (Female), `U` when missing,
  blank, not collected at all, or any other value.
- **AGE**: age in whole years as collected; blank when missing.
- **ARM**: planned arm as collected; null when none was collected.
- **ACTARM**: actual arm; in this simple example it always equals the
  planned arm (no mid-study crossover), so it is null when ARM is null.
- **ARMNRS**: reason subject not assigned to treatment;
  `Not assigned to treatment arm` when ARM is null, null otherwise.
  Per the SDTMIG 3.3+ convention, unassigned subjects have null ARM/ACTARM
  with the reason in ARMNRS (`Unassigned` is not a valid arm value).

**Note:** one record for each subject the extract carries, whichever items
that subject has. The keys **STUDYID** and **USUBJID** set that grain, so
no filter decides how many records come out, and a subject collected twice
does not become two records. Each variable names the collected item it
reads; the mapping default fills **SEX** when the subject has no usable
row for it, while **AGE**, **ARM** and **ACTARM** stay null and the
unassigned reason is recorded in **ARMNRS**.

**Standard:** SDTM | **Domain:** DM
