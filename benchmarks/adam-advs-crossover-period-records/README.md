# Place each vital-signs record in its crossover period

**Goal:** give each vital-signs record of a two-period crossover study its
analysis date and datetime (`ADT`, `ADTM`), the analysis period it falls in
(`APERIOD`), and that period's actual treatment (`TRTA`).

**Input:** Study Data Tabulation Model (SDTM) vital signs with the
collection date and time (`VSDTC`), and the subject-level analysis dataset
(ADSL) with each period's start date and datetime (`AP01SDT`, `AP01SDTM`,
`AP02SDT`, `AP02SDTM`) and actual treatment (`TRT01A`, `TRT02A`). A
subject who never started period 2 has no period-2 start or treatment.

**Variables:**

- `PARAMCD` and `AVAL` carry the test code and the numeric result.
- `ADT` is the collection date; empty when no date was collected.
- `ADTM` is the collection date and time; empty when only a date was
  collected, since no time is supplied.
- `APERIOD` is the latest period whose start is at or before the record,
  so a period runs from its start up to, but not including, the next
  period's start: a record exactly at the period 2 start is in period 2,
  one second earlier in period 1. The last period a subject started has
  no end. A record with a date but no time is placed by its date against
  the period start dates, so a date-only record on the day period 2
  starts is in period 2. Empty before the first period starts and when no
  date was collected.
- `TRTA` is the actual treatment of that period: `TRT01A` in period 1 and
  `TRT02A` in period 2; empty when the record has no period.
