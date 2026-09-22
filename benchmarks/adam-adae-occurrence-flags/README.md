# First Event Flags

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-occurrence-flags.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** mark the first treatment-emergent adverse event (AE) for each
subject with three flags: one for the subject overall, one within each
body system, and one within each dictionary-derived term.

**Input:** one record per adverse event carrying `AEBODSYS` (body
system), `AEDECOD` (dictionary-derived term), `ASTDT` (analysis start
date), and `TRTEMFL` (treatment-emergent flag: `Y` when the event is
treatment-emergent).

**Variables:**

- `AOCCFL` is `Y` for the subject's first treatment-emergent event and
  missing otherwise.
- `AOCCSFL` is `Y` for the subject's first treatment-emergent event
  within each body system and missing otherwise.
- `AOCCPFL` is `Y` for the subject's first treatment-emergent event
  within each dictionary-derived term and missing otherwise.

**Note:** first means the earliest analysis start date, with the lower
AE sequence number breaking ties on the same day. Only
treatment-emergent events are eligible, so an event that started before
treatment is never flagged at any level -- a subject with no
treatment-emergent events has all three flags missing on every record.
The levels nest: a subject's first event is also the first in its body
system and in its dictionary-derived term. Sponsors use these flags to
trace summary-table counts back to their source records.

**Standard:** ADaM | **Domain:** ADAE
