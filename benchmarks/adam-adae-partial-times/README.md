# Partial Time Imputation

**Goal:** one row per adverse event (AE) with an analysis start and end
date/time (`ASTDTM`, `AENDTM`) completed from collected times that may stop
at the hour or the minute, and the time imputation flags (`ASTTMF`,
`AENTMF`) recording which parts of the time were supplied.

**Input:** adverse event records carrying the reported term (`AETERM`) and
the collected start and end (`AESTDTC`, `AEENDTC`): a date and time to the
second, to the minute or to the hour, a date alone, a year and month alone,
or nothing at all.

**Variables:**

- `ASTDTM` is the analysis start date/time. A time collected to the second
  is used as it stands. Each part of the time that was not collected takes
  its earliest value, so a start collected at hour `14` becomes `14:00:00`
  and a date alone starts at `00:00:00`. A start collected without its day,
  or not collected at all, gives no analysis start.
- `ASTTMF` names the coarsest part of the time that was supplied: `H` when
  the hour was supplied (a date alone), `M` when the minutes were, and `S`
  when only the seconds were. It is empty when the time was collected to
  the second and when there is no analysis start.
- `AENDTM` is the analysis end date/time, completed the same way but with
  the latest value of each missing part: an end collected at hour `16`
  becomes `16:59:59` and a date alone ends at `23:59:59`.
- `AENTMF` flags `AENDTM` the way `ASTTMF` flags the start.

**Note:** a flag records what was collected, not whether the completed time
looks different. A start collected as `08:30` and one collected as
`08:30:00` complete to the same moment, but only the first is flagged `S`;
a start collected at hour `00` is flagged `M` although it completes to
the same time as a date alone.
