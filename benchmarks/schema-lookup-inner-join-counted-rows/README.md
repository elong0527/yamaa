# One Dose Record per Day of Dosing

**Goal:** give one row for each day a collected exposure (EX) record
covers, carrying the collected record's `EXSEQ`, `EXTRT`, `EXDOSE` and
`EXDOSU` through, numbering each administration in `ADOSEN` and dating it
in `ASTDT`.

**Input:** one `spec.yaml` over collected exposure records of once-daily
dosing, each with its treatment, dose, unit, and the first and last day it
covers (`EXSTDTC`, `EXENDTC`), and the protocol's dosing period, one record
per day of it, the days numbered from `1`.

**Variables:**

- `EXDOSE` is the amount given at each administration.
- `ADOSEN` numbers the administrations built from one collected record, in
  date order from `1`.
- `ASTDT` is the day of the administration: the collected record's start
  date for the first, and one day later for each one after it.

**Note:** a collected record stands for one administration on every day
from its start date through its end date, and for a single administration
on its start date when it has no end date. A record that covers more days
than the protocol's dosing period has, ends before it starts, or has no
start date rejects the run, and so does a dosing period that does not hold
each of its days once, numbered from `1`, so no record builds fewer
administrations than it stands for.
