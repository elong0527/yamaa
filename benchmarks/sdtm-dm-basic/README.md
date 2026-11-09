# Basic Demographics

**Goal:** build one Demographics (DM) record per subject: site (SITEID),
age (AGE) and its units (AGEU), sex (SEX), planned arm (ARM), actual arm
(ACTARM), reason not assigned (ARMNRS) and country (COUNTRY).

**Input:** EDC output in long form, one row per
collected item; e.g. subject 001 has **SITEID**, **COUNTRY**, **SEX**,
**AGE** and **ARM** rows.

**Variables:**

- **SITEID**: the study site identifier as collected.
- **AGE**: age in whole years as collected; blank when missing.
- **AGEU**: `YEARS` whenever **AGE** has a value; blank otherwise.
- **SEX**: recorded sex coded `M` (Male), `F` (Female), `U` when missing,
  blank, not collected at all, or any other value.
- **ARM**: planned arm as collected; null when none was collected.
- **ACTARM**: actual arm; in this simple benchmark it always equals the
  planned arm (no mid-study crossover), so it is null when ARM is null.
- **ARMNRS**: reason subject not assigned to treatment; the controlled
  term `NOT ASSIGNED` when ARM is null, null otherwise.
  Per the SDTMIG 3.3+ convention, unassigned subjects have null ARM/ACTARM
  with the reason in ARMNRS (`Unassigned` is not a valid arm value).
- **COUNTRY**: the country of the subject's site as collected, as an
  ISO 3166 three-letter code.

**Note:** one record for each subject the extract carries, whichever items
that subject has. The keys **STUDYID** and **USUBJID** are that row identity,
so no filter decides how many records come out, and a subject collected twice
does not become two records. Each variable names the collected item it
reads; the mapping default fills **SEX** when the subject has no usable
row for it, while **AGE**, **ARM** and **ACTARM** stay null and the
unassigned reason is recorded in **ARMNRS**.
