# Treatment emergence by phase in a neoadjuvant, surgery and adjuvant study

**Goal:** one row per adverse event (AE), flagging the phase in which the
event is treatment-emergent (`TREM01FL`, `TREM02FL`, `TREM03FL`) and
whether it is treatment-emergent at all (`TRTEMFL`), in an oncology study
whose phases are neoadjuvant treatment, surgery and adjuvant treatment.

**Input:** AE records carrying the reported term (`AETERM`), whether the
event is serious (`AESER`, `Y` or `N`) and the onset date (`AESTDTC`), plus
the subject-level analysis dataset (ADSL) with the first and last dates of
each phase: neoadjuvant (`TR01SDT`, `TR01EDT`), surgery (`TR02SDT`,
`TR02EDT`) and adjuvant (`TR03SDT`, `TR03EDT`), and the end-of-treatment
status (`EOTSTT`: `COMPLETED`, `DISCONTINUED` or `ONGOING`). A phase the
subject never entered has empty dates; an untreated subject has no dates
and no status.

**Variables:**

- `ASTDT` is the onset date.
- `TREM01FL` holds `Y` for an event that started in the neoadjuvant phase:
  on or after `TR01SDT` and before the next phase the subject entered
  starts, which is surgery or, for a subject who had no surgery, the first
  adjuvant dose. Empty for any other event.
- `TREM02FL` holds `Y` for an event that started in the surgery phase: on
  or after the surgery date `TR02SDT` and before the first adjuvant dose
  `TR03SDT`. Empty for any other event.
- `TREM03FL` holds `Y` for an event that started in the adjuvant phase: on
  or after the first adjuvant dose `TR03SDT`. Empty for any other event.
- `TRTEMFL` holds `Y` when one of the three phase flags is `Y`; empty
  otherwise.

**Note:** each phase starts on its own first date and ends the day before
the next phase the subject entered starts, so an event on the day of
surgery is a surgery-phase event and an event between phases belongs to
the earlier phase however long the gap. The last phase a subject entered
has no next phase to end it: it ends 30 days after its last date
(`TR01EDT`, `TR02EDT` or `TR03EDT`) for a non-serious event and 90 days
after it for a serious one, both days included, and has no end while the
subject's treatment is ongoing (`EOTSTT` is `ONGOING`). An event before
`TR01SDT`, an event after the last phase ends and every event of an
untreated subject carry no flags. The window lengths and the ongoing rule
are this contract's choices; a protocol sets its own.
