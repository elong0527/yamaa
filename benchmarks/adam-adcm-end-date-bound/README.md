# Partial End Dates Capped at Death

**Goal:** one row per concomitant medication with an analysis end date
(`AENDT`) completed from the collected end at whatever precision it was
collected, never later than a date of death that falls within the
collected month or year, and a flag (`AENDTF`) marking how much of the
date was supplied.

**Input:** concomitant medication records, each identified by its
sequence number (`CMSEQ`), carrying the reported name (`CMTRT`) and the
collected end (`CMENDTC`): a full date, a year and month, a year alone,
or nothing at all, plus the date of death (`DTHDT`) from the
subject-level analysis dataset (ADSL), empty for a subject who has not
died.

**Variables:**

- `AENDT` is the analysis end date. A fully collected date is used as it
  stands. A year and month is completed to the last day of that month,
  and a year alone to December 31 of that year. A missing end gives no
  analysis date.
- `AENDTF` is `D` when only the day was supplied and `M` when the month
  and day were supplied. It is empty when the date was collected in full
  and when no analysis date could be formed.
- `DTHDT` is the subject's date of death, empty when the subject has not
  died.

**Note:** a supplied end is never placed after a death that falls
inside the collected month or year. When the last day of the collected
month or year falls after `DTHDT`, the date moves back to the date of
death, which the collected month or year still allows, and the flag
still records what was supplied. When the whole collected month begins
after the date of death, no day it allows could reach it, so the end
stays on the last day of that month rather than moving into a month
nobody recorded. A fully collected date is left exactly as collected
even when it falls after `DTHDT`.
