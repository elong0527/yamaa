# Sentence Case and Title Case

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-sentence-case.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive `AVISIT` (sentence case) and `AVISIT_TITLE` (title
case) from the collected visit name, replacing a hand-written
visit-label mapping.

**Input:** lab records carrying `VISIT` (the collected visit name, e.g.
`WEEK 8`).

**Variables:**

- `VISIT` is the collected visit name, carried through unchanged.
- `AVISIT` is `VISIT` in sentence case: the first character becomes
  uppercase and every later character becomes lowercase. Only plain
  English letters change; every other character is unchanged.
- `AVISIT_TITLE` is `VISIT` in title case: the first letter of each
  word becomes uppercase and the remaining letters of each word become
  lowercase. Only plain English letters change; every other character
  is unchanged.

The two agree on single-word labels (`WEEK 8` gives `Week 8` both
ways) and differ on multi-word labels: `END OF TREATMENT` becomes
`End of treatment` in sentence case but `End Of Treatment` in title
case.
