# Adverse Events Within 24 Hours After a Dose

**Goal:** mark each adverse event (AE) that started within 24 hours after
the start of one of the subject's doses (`DOSE24FL`), and carry the number
of that dose (`DOSENUM`).

**Input:** AE records carrying the coded term (`AEDECOD`) and the collected
onset (`AESTDTC`, a date with or without a time of day), plus one exposure
(EX) record per dose with the date and time the dose started (`EXSTDTC`).

**Variables:**

- `ASTDT` is the onset date; empty when no onset was collected.
- `ASTDTM` is the onset date and time, filled only when a time of day was
  collected. No time is imputed, so an onset recorded as a date alone
  stays empty here.
- `DOSE24FL` holds `Y` when the onset is at or after a dose start and
  before 24 hours have passed since that start: an onset exactly at the
  start is inside, one exactly 24 hours later is outside. An onset before
  the first dose, an onset without a time and a missing onset stay empty.
- `DOSENUM` is the number of the dose whose 24 hours hold the onset,
  counting the subject's doses 1, 2, 3 in order of start; empty when
  `DOSE24FL` is empty. When two doses start less than 24 hours apart and
  the onset falls within both, the later dose is counted.

**Note:** the 24-hour length, the open end at exactly 24 hours and counting
the later of two overlapping doses are this contract's stated choices.
Times compare to the second; a time collected without seconds counts as
second zero.
