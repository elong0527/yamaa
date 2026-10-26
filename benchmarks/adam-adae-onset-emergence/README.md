# Treatment Emergence Timing

**Goal:** one row per adverse event (AE), marking treatment
emergence (`TRTEMFL`) and the subject's earliest treatment-emergent
event (`AOCCFL`) from onset, first exposure, and the study's
post-last-exposure emergence window: 30 days for a non-serious event,
90 days for a serious one.

**Input:** adverse event records carrying the reported term
(`AETERM`), whether the event was serious (`AESER`, `Y` or `N`, empty
when not recorded) and the collected onset moment (`ASTDTM`, empty
when never collected), plus, from the subject-level analysis dataset
(ADSL), the moments of first and last exposure (`TRTSDTM`, `TRTEDTM`)
and the end-of-treatment status (`EOTSTT`: `COMPLETED`,
`DISCONTINUED` or `ONGOING`). While treatment is ongoing, `TRTEDTM`
holds the last exposure so far. An untreated subject appears in ADSL
with an empty `TRTSDTM` and none of their events get flagged.

**Variables:**

- `TEENDDTM` is the end of the treatment-emergent window: 30 days
  after last exposure for an event recorded as non-serious, and 90
  days after for any other event, serious or with its seriousness
  not recorded. It stays empty while treatment is ongoing, because
  the window has no end yet, and for an untreated subject.
- `TRTEMFL` holds `Y` when the event started inside the window  - 
  at or after first exposure and at or before the window end, or at
  any time after first exposure while treatment is ongoing. It stays
  empty for an earlier or later event. A missing onset cannot be
  ruled pre-treatment, so the conservative convention flags it `Y`
  whenever the subject was treated; an untreated subject's events
  stay empty.
- `AOCCFL` holds `Y` on the subject's earliest treatment-emergent
  event, ordered by onset moment with the lower sequence number
  (`AESEQ`) settling ties at the same second. All other rows stay
  empty; a treatment-emergent event with a missing onset is flagged
  but cannot be ordered, so it never carries the occurrence flag.

**Note:** emergence is decided at the moment, not the day: an
event that started earlier on the day of first exposure and one
that started later that same day fall on opposite sides, and a
start date alone cannot tell them apart. Both window ends belong to
the window, and a day is 24 hours, so a serious event one second
past 90 days after last exposure is outside it.

**Note:** a collected datetime without seconds is coerced to the
second (seconds zero-filled) and compared at second precision.
HEADACHE is collected as `2025-03-04T11:45` and VOMITING as
`2025-03-04T11:45:00`; both land on the same second, so the
`AOCCFL` tie-break ("the lower sequence number settles ties at
the same second") is what picks the earliest treatment-emergent
event. No time-imputation flag is derived for the coerced value:
the onset is read directly as a date and time, which takes the
omitted seconds as collected zeros rather than as supplied ones.
