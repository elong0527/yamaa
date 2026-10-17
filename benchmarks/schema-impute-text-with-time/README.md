# Analysis Start Date From a Collected Date and Time

**Goal:** one row per adverse event (AE) with an analysis start date
(`ASTDT`) taken from a collected start that may carry a time of day, and a
flag (`ASTDTF`) marking how much of the date was supplied.

**Input:** one `spec.yaml` over adverse event records carrying the reported
term (`AETERM`) and the collected start (`AESTDTC`): a date with a time to the
second, minute or hour, a date alone, a year and month, a year alone, or
nothing at all.

**Variables:**

- `ASTDT` is the analysis start date. A start carrying a complete date gives
  that calendar date, and its time of day plays no part. A year and month is
  completed to the first of the month, and a year alone to 1 January. A start
  never collected gives no analysis date.
- `ASTDTF` is `D` when the day was supplied and `M` when the month and day
  were supplied. It is empty when the date was collected in full, with or
  without a time, and when there is no analysis date.

**Note:** collected text is read in every truncated form the standard
allows, so a start that carries a time completes to its calendar date
exactly as a date alone does.
