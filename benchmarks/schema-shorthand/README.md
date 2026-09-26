# Shorthand Spellings

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-shorthand.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** show that a lone value may stand where a list is accepted,
and a lone condition or label string may stand where its full form is
accepted: the short spellings run exactly as their long forms.

**Input:** one `spec.yaml` over a demographics file (one record per
subject: sex and age) and a weight file (several dated weight records
per subject).

**Variables:**

- `SEX`: the subject's recorded sex.
- `AGE`: the subject's age in years; blank when not collected.
- `AGE65FL`: `Y` for subjects aged 65 or older; blank otherwise. A
  lone condition needs no explicit yes or no text: it answers `Y`
  when true and stays blank when false or unknown.
- `SUBJLBL`: a readable label joining the subject identifier and sex,
  written as one plain string with placeholders.
- `WTMAX`: the largest collected weight across the subject's weight
  records; the grouping key is written as a single key.
- `LASTWT`: the weight from the subject's latest dated record; the
  latest record wins because a single ordering value sorts
  earliest-first, and the match key is written as a single key.

**Note:** each short spelling expands to its full form before the
run: a lone key becomes a one-key list, a lone check becomes a
one-check list, a lone condition answers `Y` when true and stays
blank otherwise, a lone label string keeps its placeholders, and a
lone ordering value sorts earliest-first.

**Standard:** ADaM | **Domain:** ADSL
