# Worsening Emergence

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-worsening-emergence.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one row per adverse event (AE), marking treatment
emergence (`TRTEMFL`) counting a pre-treatment event that worsens
after first exposure.

**Input:** adverse event records carrying the dictionary term
(`AEDECOD`), the onset moment (`ASTDTM`, empty when never
collected) and severity (`AESEV`), plus the moment of first
exposure (`TRTSDTM`) from the subject-level analysis dataset
(ADSL), which is empty for an untreated subject.

**Variables:**

- `TRTEMFL` holds `Y` when the event started at or after first
  exposure, or when it started before but the same term reaches a
  higher severity after exposure. It stays empty for an earlier
  event without later worsening, and when the onset, the severity,
  or the first exposure is missing.

**Note:** worsening is judged within the subject and dictionary
term: only a later event for the same term with a higher severity
makes the earlier one treatment-emergent, and an untreated
subject's events are never flagged.

**Standard:** ADaM | **Domain:** ADAE
