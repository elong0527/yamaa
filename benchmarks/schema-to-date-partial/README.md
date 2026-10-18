# Analysis Start Date From Complete Collected Dates Only

**Goal:** one row per concomitant medication with an analysis start date
(`ASTDT`) taken only from a start collected with a complete date.

**Input:** one `spec.yaml` over concomitant medication records, each
identified by its sequence number (`CMSEQ`), carrying the reported name
(`CMTRT`) and the collected start (`CMSTDTC`): a date with a time to the
minute or the hour, a date alone, a year and month, a year alone, or nothing
at all.

**Variables:**

- `ASTDT` is the calendar date of the collected start when it carries a
  complete date, whether or not a time follows it. A start collected to the
  year and month or the year alone is partial and gives no analysis date,
  and nothing is supplied to complete it; a start never collected gives
  none either.

**Note:** a partial start is not an error here: it is counted and left
without a date, while text that is not a date at all would stop the run.
