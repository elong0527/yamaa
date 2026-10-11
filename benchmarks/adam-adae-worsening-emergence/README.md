# Worsening Emergence

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-worsening-emergence.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one row per adverse event (AE), marking treatment
emergence (`TRTEMFL`) counting a pre-treatment event that continues
and worsens after first exposure.

**Input:** adverse event records carrying the dictionary term
(`AEDECOD`), onset and end moments (`AESTDTC`, `AEENDTC`, empty
when uncollected), severity (`AESEV`) and toxicity grade
(`AETOXGR`, empty when never graded), plus the moment of first exposure
(`TRTSDTM`) from the subject-level analysis dataset (ADSL),
which is empty for an untreated subject.

**Variables:**

- `ASTDTM` holds the collected onset moment; empty when missing.
- `AENDTM` holds the collected end moment; empty when missing.
- `AETOXGR` holds the collected toxicity grade (1 through 5);
  empty when the event was never graded.
- `TRTEMFL` holds `Y` when the event started at or after first
  exposure, or when it started before but its continuous chain reaches
  a higher severity or a higher toxicity grade after exposure. It
  stays empty for an earlier event without later worsening, and
  when the onset or the first exposure is missing.

**Note:** within each study, subject and dictionary term, order episodes
by onset then `AESEQ`. Consecutive episodes belong to the same chain
only when the preceding end equals the next onset exactly. A gap,
overlap or missing boundary breaks the chain. Unchanged episodes can
link later worsening; missing severity or grade shows no worsening
for that dimension. Untreated subjects' events are never flagged.

**Standard:** ADaM | **Domain:** ADAE
