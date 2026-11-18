# Two Dose Records per Day of Twice-Daily Dosing

**Goal:** give one row for each administration of a collected exposure (EX)
record of twice-daily dosing, carrying the collected record's `EXSEQ`,
`EXTRT`, `EXDOSE` and `EXDOSU` through, numbering each administration in
`ADOSEN`, dating it in `ASTDT` and numbering it within its day in `ATPTN`.

**Input:** one `spec.yaml` over collected exposure records of twice-daily
dosing, each with its treatment, dose, unit, and the first and last day it
covers (`EXSTDTC`, `EXENDTC`).

**Variables:**

- `EXDOSE` is the amount given at each administration.
- `ADOSEN` numbers the administrations built from one collected record, in
  time order from `1`.
- `ASTDT` is the day of the administration: the collected record's start
  date for the first two, and one day later for each two after them.
- `ATPTN` is `1` for the first administration of a day and `2` for the
  second.

**Note:** a collected record stands for two administrations on every day
from its start date through its end date, so it builds twice as many rows
as the days it covers, a number worked out from its own dates.
