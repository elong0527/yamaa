# Create DM from EDC extract

**Goal:** derive DM common variables **SEX**, **AGE**, **ARM** and **ACTARM**.

**Input:** one row per collected item from EDC (ODM XML); e.g. subject
001 has **SEX**, **AGE** and **ARM** rows.

**Variables:**

- **SEX**: Male to `M`, Female to `F`; missing, blank, or other value
  becomes `U`.
- **AGE**: integer years as collected; blank when missing.
- **ARM**: planned arm text; `Unassigned` when none collected.
- **ACTARM**: actual arm; here always equals **ARM**.

**Note:** one row per subject with a **SEX** record, even if blank;
fallbacks fill the rest.

**Standard:** SDTM | **Domain:** DM

[Rendered view](https://elong0527.github.io/yamaa/examples/sdtm-dm-basic.html)
