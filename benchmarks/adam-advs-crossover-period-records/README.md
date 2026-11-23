# Place each vital-signs record in its crossover period

**Goal:** give each vital-signs record of a two-period crossover study its
analysis date and datetime (`ADT`, `ADTM`), the analysis period it falls in
(`APERIOD`), and that period's actual treatment (`TRTA`).

**Input:** Study Data Tabulation Model (SDTM) vital signs with the
collection date and time (`VSDTC`), and the subject-level analysis dataset
(ADSL) with each period's start and end dates and datetimes (`AP01SDT`,
`AP01SDTM`, `AP01EDT`, `AP01EDTM`, `AP02SDT`, `AP02SDTM`, `AP02EDT`,
`AP02EDTM`) and actual treatment (`TRT01A`, `TRT02A`). A period's datetime
is empty when no time was recorded for it, and a subject who never started
period 2 has no period-2 dates or treatment.

**Variables:**

- `PARAMCD` and `AVAL` carry the test code and the numeric result.
- `ADT` is the collection date; empty when no date was collected.
- `ADTM` is the collection date and time; empty when only a date was
  collected, since no time is supplied.
- `APERIOD` is the period whose start and end the record lies within,
  both included. Each limit is compared by time when both the record and
  the limit have a time, and by date otherwise: a record at 10:00 on the
  day period 1 ends at 08:00 is after period 1, while a date-only record
  on that day is in period 1, as is a timed record against a period with
  no recorded times. Empty for a record between the periods, before the
  first or after the last, and when no date was collected.
- `TRTA` is the actual treatment of that period: `TRT01A` in period 1 and
  `TRT02A` in period 2; empty when the record has no period.

**Note:** a period with no start or end date holds no record. When a
date-only record falls on a day that ends one period and starts the next,
it is placed in the later period.
