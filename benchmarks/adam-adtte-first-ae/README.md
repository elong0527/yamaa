# Measure Time to the First Adverse Event

**Goal:** build one `TTAE` (Time to First Adverse Event) record
for each treated subject, counting the days from treatment start
to the first adverse event, or to the censoring date when no event
with a usable onset date exists.

**Input:** one row per subject with treatment start (`TRTSDT`),
end-of-study (`EOSDT`) and last known alive (`LSTALVDT`) dates,
plus adverse event records carrying onset dates (`ASTDT`).

**Variables:**

- `STARTDT`: treatment start date.
- `ADT`: the earliest adverse event onset date. When no event
  carries a usable onset date, it is the end-of-study date, or the
  last known alive date for a subject who has not ended the study.
  A date before treatment start is moved up to it.
- `AVAL`: whole days from `STARTDT` through `ADT`, counting both
  endpoints.
- `CNSR`: `0` when the record marks an event, `1` when the
  subject is censored.
- `EVNTDESC`: `AE` for an event; `END OF STUDY` or `LAST KNOWN
  ALIVE` for a censored record, naming the date it is censored at.
- `SRCDOM`: `ADAE` for an event, `ADSL` for a censored record.
- `SRCVAR`: `ASTDT` for an event, and `EOSDT` or `LSTALVDT` for a
  censored record.
- `SRCSEQ`: the chosen event's sequence number: the lower
  number when several events share the earliest onset date;
  blank for a censored record.

**Note:** a subject who never started treatment has no record, so
every record has a day count. An event with no onset date cannot
start the clock: it is passed over when a dated event exists, and
when it is the only event the subject is censored. Moving a date up
to the treatment start changes only the date: the record keeps its
event-or-censoring status and source, and the day count is `1`.
