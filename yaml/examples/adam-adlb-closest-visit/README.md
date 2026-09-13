# Select the record closest to a window's target day

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adlb-closest-visit.html)

**Goal:** derive `AVISIT`, `AWTARGET`, `ADIST`, and `ANL01FL` to
place each laboratory record in its analysis visit and flag the
record closest to the visit's target day.

**Input:** one record per laboratory measurement, carrying `ADT`
(analysis date), `ADY` (study day), and `AVAL` (measured result).

**Variables:**

- `AVISIT` is the analysis visit the record falls in (`WEEK 2`
  covers study days 8 through 22) and missing when the record
  falls outside every window.
- `AWTARGET` is the study day the visit aims at (day 15 for
  `WEEK 2`) and missing when the record falls outside every
  window.
- `ADIST` is how far the record's study day lies from that
  target, in days and without direction, and missing when the
  record falls outside every window.
- `ANL01FL` is `Y` for the record that stands for its subject and
  parameter in the visit: the closest to the target, or the one
  with the later study day when two are equally close. It is
  missing otherwise, and a record outside every window is never
  flagged.

**Note:** the three window columns travel together: a record
inside the window carries all three, while a record outside every
window carries none and is never flagged, so every input record
stays in the output and the distance beside each flag shows why
that record was chosen.

**Standard:** ADaM | **Domain:** ADLB
