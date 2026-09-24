# First Rescue Medication

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adsl-rescue-med.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive the first rescue medication for each subject,
carried in `RESCTRT`.

**Input:** demographics records listing the subjects, plus
medication records with treatment name, category, start date, and
sequence number.

**Variables:**

- `RESCTRT` is the treatment name (`CMTRT`) of the subject's
  earliest rescue medication, a record whose category (`CMCAT`) is
  `RESCUE MEDICATION`. Earliest means the smallest start date
  (`CMSTDTC`), and ties go to the smallest `CMSEQ`. Left blank, not
  filled with placeholder text, when the subject has no rescue
  medication, whether the subject has other medications or none.

**Standard:** ADaM | **Domain:** ADSL
