# Treatment Emergence Timing

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/adam-adae-onset-emergence.html)
[![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** one row per adverse event (AE), marking treatment
emergence (`TRTEMFL`) and the subject's earliest treatment-emergent
event (`AOCCFL`) from onset and first-exposure moments.

**Input:** adverse event records carrying the reported term
(`AETERM`) and the collected onset moment (`ASTDTM`, empty when never
collected), plus the moment of first exposure (`TRTSDTM`) from the
subject-level analysis dataset (ADSL), which is empty for an untreated
subject; an untreated subject appears in ADSL with an empty `TRTSDTM`
and none of their events get flagged.

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

**Note:** a collected datetime without seconds is coerced to the
second (seconds zero-filled) and compared at second precision.
HEADACHE is collected as `2025-03-04T11:45` and VOMITING as
`2025-03-04T11:45:00`; both land on the same second, so the
`AOCCFL` tie-break ("the lower sequence number settles ties at
the same second") is what picks the earliest treatment-emergent
event. No time-imputation flag is derived for the coerced value:
the coercion happens inside the engine's datetime type, and the
language's collected-precision model reports only day/second
tiers, so a minute-precision source is not distinguishable from
a second-precision one in spec language.

**Standard:** ADaM | **Domain:** ADAE
