# Analysis Start Date Kept Within Treatment Start and the Event's End

**Goal:** one row per adverse event (AE) with an analysis end date
(`AENDT`), and an analysis start date (`ASTDT`) completed from a partial
start no earlier than first exposure and no later than the event's end, and
a flag (`ASTDTF`) marking how much of the start date was supplied.

**Input:** one `spec.yaml` over adverse event records carrying the reported
term (`AETERM`) and the collected start and end (`AESTDTC`, `AEENDTC`): a
full date, a year and month, a year alone, or no end at all, plus the first
exposure date (`TRTSDT`) from the subject-level analysis dataset (ADSL).

**Variables:**

- `TRTSDT` is the subject's first exposure date.
- `ASTDT` is the analysis start date. A fully collected start is used as it
  stands. A partial start takes the first day it allows, then moves up to
  `TRTSDT` and back to `AENDT` wherever either date falls inside the
  collected month or year; a date outside it moves nothing. When both fall
  inside and first exposure is the later, the start takes the end date, so a
  start is never placed after the event ended.
- `ASTDTF` is `D` when the day was supplied and `M` when the month and day
  were supplied, whether or not first exposure or the end then moved the
  date. It is empty when the start was collected in full.
- `AENDT` is the analysis end date: a fully collected end as it stands, a
  year and month completed to the last day of that month, and a year alone
  to 31 December; empty when no end was collected.

**Note:** when first exposure and the end both fall inside the collected
period and cross, the end wins, so the start never follows the end it
belongs to and is never left empty.
