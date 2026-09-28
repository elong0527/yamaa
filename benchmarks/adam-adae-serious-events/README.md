# Keep Only Serious Events

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-serious-events.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one Analysis Dataset for Adverse Events (ADAE) row per
serious adverse event (AE), carrying `AEDECOD` and `AESER`.

**Input:** collected adverse events with study, subject, and
sequence (`STUDYID`, `USUBJID`, `AESEQ`), the dictionary term
(`AEDECOD`), and whether the event was serious (`AESER`).

**Variables:**

- `AEDECOD` is the dictionary term collected for the event.
- `AESER` is `Y` on every row, because only serious events are
  kept.

**Note:** rows follow the order in which the events were
collected, so the serious events of one subject stay interleaved
with the serious events of other subjects exactly as collected. A
non-serious event leaves no row.

**Standard:** ADaM | **Domain:** ADAE
