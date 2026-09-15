# Create DM from EDC extract

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-dm-basic.html)

**Goal:** build one Demographics (DM) record per subject: sex (SEX), age
(AGE), planned arm (ARM) and actual arm (ACTARM).

**Input:** EDC output in long form, one row per collected item; e.g.
subject 001 has **SEX**, **AGE** and **ARM** rows. The `input:` block names
the single source dataset, **ODM**.

**Grain:** `keys: [STUDYID, USUBJID]` declares the grain, and each key's
`source:` derivation builds the key relation from ODM rows: `STUDYID` from
`ODM.StudyOID`, `USUBJID` from `ODM.SubjectKey`.

**Columns:** each value column carries a filtered `source:` derivation
that selects the matching item for the key tuple -- e.g. **SEX** maps the
`ODM.Value` rows where `ODM.ItemOID = 'IT.DM.SEX'` through a `Male/Female`
dictionary, **AGE** reads the `'IT.DM.AGE'` rows as an integer, and **ARM**
coalesces the `'IT.DM.ARM'` rows with `Unassigned` as the default.
**ACTARM** copies **ARM**. Subjects with no matching rows get the column's
missing handling, so the four subjects appear with **SEX** `U` and empty
**AGE** where items were not collected.

There is no `rows:` block, no aggregate expression, and no per-column
ODM item references -- correlation happens by recomputed key tuples.

**Standard:** SDTM | **Domain:** DM
