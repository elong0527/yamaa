# Infusion duration from the collected duration or the start and end times

**Goal:** give each infusion its duration in minutes as `AVAL`, with the
unit in `AVALU`, and carry beside it `CALCDUR`, the duration computed from
the start and end times.

**Input:** Study Data Tabulation Model (SDTM) exposure (EX) records, one
per infusion (`EXSEQ`), with the start (`EXSTDTC`) and end (`EXENDTC`) as
ISO 8601 date or date-time text and, where the case report form collected
it, the infusion's duration as ISO 8601 duration text (`EXDUR`, for example
`PT1H30M` for one hour 30 minutes).

**Variables:**

- `AVAL` is the collected duration in minutes when `EXDUR` holds a readable
  fixed-length ISO 8601 duration; otherwise it is `CALCDUR`. Text that is
  not such a duration, for example one without its leading `P`, counts as
  not collected. Empty when neither gives a value.
- `CALCDUR` is the elapsed time in minutes from the start to the end, with
  seconds kept as a fraction of a minute and nothing added or rounded. It is
  empty when the start or the end was collected as a date without a time of
  day; no time is assumed.

**Note:** a readable collected duration always wins, even when it disagrees
with the start and end times. `CALCDUR` keeps the computed value beside it
so the two can be compared. Every infusion keeps its record, with an empty
`AVAL` when its duration cannot be known.
