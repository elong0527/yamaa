# Pre-specified Medical History Checklist

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/sdtm-mh-prespecified-conditions.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** map a pre-specified medical history checklist into MH and carry
any number of volunteered free-text conditions alongside it.

**Input:** the ODM extract has one item record for each asked checklist
condition, including an unanswered condition with a blank value. A small
item-definition table gives each checklist item its reported term and form
order. Each volunteered condition has its own repeated free-text record.
Repeat numbers can be reused at another visit.

**Variables:**

- `MHTERM` is the checklist condition from the item-definition table, or the
  volunteered text exactly as reported.
- `MHCAT` distinguishes disease-specific checklist history from general
  volunteered history.
- `MHPRESP` is `Y` for checklist records and blank for volunteered records.
- `MHOCCUR` is `Y` or `N` for an answered checklist condition and blank
  otherwise.
- `MHSTAT` is `NOT DONE` for an unanswered checklist condition and blank
  otherwise.
- `MHSEQ` orders checklist conditions as shown on the form, then volunteered
  conditions by visit and form repeat.

**Note:** an unanswered checklist item still has an ODM record. A question
entirely absent from the extract is not assumed to have been asked. Two
volunteered conditions at one visit, and another condition whose repeat
number is reused at a later visit, remain three distinct MH records.

**Standard:** SDTM | **Domain:** MH
