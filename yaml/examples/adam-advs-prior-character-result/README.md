# Carry forward the latest earlier character result

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-advs-prior-character-result.html)

**Goal:** build one ADVS row per collected character-result row,
carrying `SERIES`, `AVISITN`, and `AVALC` through unchanged and
adding `PREVAVALC` for the closest earlier non-blank result.

**Input:** collected character results with series, visit number,
and character result.

**Variables:**

- `SERIES` identifies the analysis series and is carried through
  unchanged; rows with no series value share one series.
- `AVISITN` is the visit number used to order rows within a
  series and is carried through unchanged; rows with a missing
  visit number sort after numbered visits.
- `AVALC` is the current character result, kept as collected;
  blank when no result was collected.
- `PREVAVALC` is the closest earlier non-blank result in the same
  series, ignoring the current row; blank at the start of a
  series.

**Note:** ordering within a series is by visit number, with
missing numbers last; the look-back skips blank results, so it
can cross consecutive blank rows, but never crosses series.

**Standard:** ADaM | **Domain:** ADVS
