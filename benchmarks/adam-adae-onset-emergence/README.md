# Treatment Emergence Timing

**Goal:** one row per adverse event (AE), marking treatment
emergence (`TRTEMFL`) and the subject's earliest treatment-emergent
event (`AOCCFL`) from onset, first exposure, and the study's
30-day post-last-exposure emergence window.

**Input:** adverse event records carrying the reported term
(`AETERM`) and the collected onset moment (`ASTDTM`, empty when never
collected), plus the moments of first and last exposure (`TRTSDTM`,
`TRTENDTM`) from the subject-level analysis dataset (ADSL); the
emergence window end (`TEENDDTM`, last exposure plus 30 days) is
carried alongside. An untreated subject appears in ADSL with an empty
`TRTSDTM` and none of their events get flagged.

**Variables:**

- `TEENDDTM` is the end of the treatment-emergent window: 30 days
  after last exposure.
- `TRTEMFL` holds `Y` when the event started inside the window  - 
  at or after first exposure and at or before the window end. It
  stays empty for an earlier or later event. A missing onset cannot
  be ruled pre-treatment, so the conservative convention flags it
  `Y` whenever the subject was treated; an untreated subject's
  events stay empty.
- `AOCCFL` holds `Y` on the subject's earliest treatment-emergent
  event, ordered by onset moment with the lower sequence number
  (`AESEQ`) settling ties at the same second. All other rows stay
  empty; a treatment-emergent event with a missing onset is flagged
  but cannot be ordered, so it never carries the occurrence flag.

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
the onset is read directly as a date and time, which takes the
omitted seconds as collected zeros rather than as supplied ones.
