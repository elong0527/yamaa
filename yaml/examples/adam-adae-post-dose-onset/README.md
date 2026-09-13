# Treatment emergence by onset moment

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adae-post-dose-onset.html)

**Goal:** one row per adverse event (AE), marking treatment
emergence (`TRTEMFL`) and the subject's earliest treatment-emergent
event (`AOCCFL`) from onset and first-exposure moments.

**Input:** adverse event records carrying the reported term
(`AETERM`) and the collected onset moment (`ASTDTM`, empty when never
collected), plus the moment of first exposure (`TRTSDTM`) from the
subject-level analysis dataset (ADSL), empty with no subject record.

**Variables:**

- `TRTEMFL` holds `Y` when the event started at or after first
  exposure. It stays empty for an earlier event, and when either
  the onset or the first exposure is missing.
- `AOCCFL` holds `Y` on the subject's earliest treatment-emergent
  event, ordered by onset moment with the lower sequence number
  (`AESEQ`) settling ties at the same second. All other rows stay
  empty.

**Note:** emergence is decided at the moment, not the day: an
event that started earlier on the day of first exposure and one
that started later that same day fall on opposite sides, and a
start date alone cannot tell them apart.

**Standard:** ADaM | **Domain:** ADAE
