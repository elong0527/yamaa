# Create DM from EDC extract

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-dm-basic.html)

**Goal:** build one Demographics (DM) record per subject: sex (SEX), age
(AGE), planned arm (ARM) and actual arm (ACTARM).

**Input:** EDC output in long form, one record per collected item; e.g. subject
001 has **SEX**, **AGE** and **ARM** information.

**Grain:** `STUDYID` and `USUBJID` identify each subject. The study and
subject identifiers accompany every collected item.

**Variables:**
- **SEX** holds `M` or `F` for a reported male or female value and `U` when
  sex is absent or not reported.
- **AGE** holds the collected integer age or is empty when age is absent.
- **ARM** holds the collected planned arm or `Unassigned` when it is absent.
- **ACTARM** copies **ARM**.

Each output value is matched to the subject's collected item using the study
and subject identifiers.

**Standard:** SDTM | **Domain:** DM
