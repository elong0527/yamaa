# Flag the first adverse event at three levels

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-occurrence-flags.html)

**Goal:** derive `AOCCFL`, `AOCCSFL`, and `AOCCPFL` to mark the first
treatment-emergent adverse event (AE) for each subject overall, within
each body system, and within each dictionary-derived term.

**Input:** one record per adverse event carrying `AEBODSYS` (body
system), `AEDECOD` (dictionary-derived term), `ASTDT` (analysis start
date), and `TRTEMFL` (`Y` when treatment-emergent).

**Variables:**

- `AOCCFL` is `Y` for the subject's earliest treatment-emergent
  event and missing otherwise.
- `AOCCSFL` is `Y` for the subject's earliest treatment-emergent
  event within each body system and missing otherwise.
- `AOCCPFL` is `Y` for the subject's earliest treatment-emergent
  event within each dictionary-derived term and missing otherwise.

**Note:** earliest means by analysis start date, with the lower AE
sequence number breaking ties on the same day. Only treatment-emergent
events are eligible, so an event that is not treatment-emergent is
never flagged at any level. The levels nest: the subject's first event
is also the first in its body system and term.

**Standard:** ADaM | **Domain:** ADAE
