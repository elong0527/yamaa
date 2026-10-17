# Date and Time Imputation Flags of an Analysis Start

**Goal:** one row per adverse event (AE) with an analysis start date/time
(`ASTDTM`) completed from the collected start, and the date and time
imputation flags (`ASTDTF`, `ASTTMF`) naming the first part of each that
was supplied.

**Input:** one `spec.yaml` over adverse event records carrying the reported
term (`AETERM`) and the collected start (`AESTDTC`), collected to the year,
month, day, hour, minute or second.

**Variables:**

- `ASTDTM` is the analysis start date/time. Every part the start did not
  collect takes its earliest value: January, the first day, and `00` for
  each part of the time.
- `ASTDTF` is `M` when the month and day were supplied and `D` when only
  the day was; empty when the date was collected in full.
- `ASTTMF` is `H` when the whole time was supplied, which includes every
  start collected only to a year, a month or a day, `M` when the minutes
  and seconds were and `S` when only the seconds were; empty when the time
  was collected to the second.

**Note:** both flags are read from the completed date/time, which keeps how
much of it was collected; the completed values themselves look alike.
