# Calendar Duration and Interval Text Between Two Dates

**Goal:** write how long before the first dose each adverse event began as
ISO 8601 duration text in years, months and days (`AEPREDUR`), and the
timing text that ends that duration at the first dose date (`AETIMING`).

**Input:** one `spec.yaml` over adverse event records that began on or
before the day of the subject's first dose, each with the event's start date
(`AESTDTC`) and the date of first dose (`RFXSTDTC`), held as dates. An
event's start may not have been collected, and a subject may never have been
dosed.

**Variables:**

- `AETERM` is the adverse event as reported.
- `AEPREDUR` is the calendar time from the event's start to the first dose:
  the whole months, written as years and months, then the days left over, so
  26 August 2007 to 27 September 2007 is `P1M1D`, one month to 26 September
  and one day more. A part that is zero is left out, and an event that began
  on the day of first dose is `P0D`. Empty when either date is missing.
- `AETIMING` is that duration, a slash and the first dose date, such as
  `P1M1D/2007-09-27`: the period of that length that ends on the first dose
  date. Empty when the duration is.

**Note:** a month is a calendar month counted from the start date, never 30
days, and when that month has no such day its last day stands in, so 31
January 2008 to 29 February 2008 is `P1M`. The same number of days therefore
gives different text from different start dates, which is why the duration
is read from the two dates and never from a count of days. The timing text
is the duration, a slash and the first dose date.
