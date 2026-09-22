# Pre-specified Medical History Checklist

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-mh-prespecified-conditions.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** map a pre-specified medical history checklist into MH,
recording for each asked condition whether the subject had it,
and carrying volunteered free-text conditions alongside.

**Input:** a checklist form with one column per asked condition
(`Y`, `N`, or blank), and a free-text form with one row per
volunteered condition.

**Variables:**

- `MHTERM` is the asked condition, or the volunteered term as
  reported.
- `MHCAT` separates the checklist (disease-specific history:
  `DISEASE-SPECIFIC HISTORY`) from the volunteered free-text
  records (general history: `GENERAL HISTORY`).
- `MHPRESP` is `Y` for checklist records, and blank for
  volunteered ones, since they were not pre-specified.
- `MHOCCUR` is the checklist answer: `Y` or `N`. It stays blank
  for volunteered conditions and for unanswered questions.
- `MHSTAT` is `NOT DONE` for an unanswered checklist question,
  and blank otherwise.
- `MHSEQ` numbers the records within each subject: checklist
  records first in form order, then volunteered ones.

**Note:** a blank checklist answer still yields a record. The
question was asked but not answered, so `MHOCCUR` stays blank
while `MHSTAT` records `NOT DONE`.

**Standard:** SDTM | **Domain:** MH
