# Retain the Latest Earlier Character Result

**Goal:** build one ADVS row per collected character-result row,
carrying `SERIES`, `AVISITN`, and `AVALC` through unchanged and
adding `PREVAVALC` for the closest earlier non-blank result.

**Input:** collected character results for three subjects, with
series, visit number, and character result.

**Variables:**

- `SERIES` identifies the analysis series; rows with no series value
  share one series.
- `AVISITN` is the visit number that orders rows within a series.
- `AVALC` is the current character result, kept as collected; blank
  when no result was collected.
- `PREVAVALC` is the closest earlier non-blank result for the same
  subject and series, never the row's own; blank when no earlier row
  in the series has a result.

**Note:** within a series, rows are ordered by visit number with
missing numbers last, and rows sharing a visit number (or both
missing one) keep their collected order. The look-back skips blank
results, so it can cross consecutive blank rows, but it never
crosses into another series. A subject with a single collected row,
and a series whose earlier rows are all blank, both leave `PREVAVALC`
blank.
