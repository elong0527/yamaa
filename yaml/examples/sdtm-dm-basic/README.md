# Create DM from EDC extract

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-dm-basic.html)

**Goal:** build one Demographics (DM) record per subject: sex (SEX), age
(AGE), planned arm (ARM) and actual arm (ACTARM).

**Input:** EDC output in long form, one row per
collected item; e.g. subject 001 has **SEX**, **AGE** and **ARM** rows.

**Variables:**

- **SEX**: recorded sex coded `M` (Male), `F` (Female), `U` when missing,
  blank, not collected at all, or any other value. The collected item is
  read into a working column, **SEXRAW**, that the result does not carry.
- **AGE**: age in whole years as collected; blank when missing.
- **ARM**: planned arm as collected; `Unassigned` when none was collected.
- **ACTARM**: actual arm; in this simple example it always equals the
  planned arm (no mid-study crossover).

**Note:** one record for each subject the extract carries, whichever items
that subject has. The keys **STUDYID** and **USUBJID** set that grain, so
no filter decides how many records come out, and a subject collected twice
does not become two records. Defaults fill **SEX**, **AGE**, **ARM** and
**ACTARM** when the item was not collected.

**Standard:** SDTM | **Domain:** DM
