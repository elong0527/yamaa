# Select the first rescue medication

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adsl-rescue-medication.html)

**Goal:** derive the first rescue medication for each subject,
carried in `RESCTRT`.

**Input:** demographics records listing the subjects, plus
medication records with treatment name, category, start date, and
sequence number.

**Variables:**

- `RESCTRT` is the treatment name from `CMTRT` on the earliest
  qualifying medication record, where qualifying means `CMCAT`
  equals `RESCUE MEDICATION`; any other category does not qualify.
  Earliest means the smallest `CMSTDTC`, and ties go to the
  smallest `CMSEQ`. Left blank when the subject has no qualifying
  record, whether the subject has other medications or none at all,
  not filled with placeholder text.

**Standard:** ADaM | **Domain:** ADSL
