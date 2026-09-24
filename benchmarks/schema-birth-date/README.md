# Birth Date Completion

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-birth-date.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** complete a collected birth date to the first and last
day of its month and read how much was collected.

**Input:** subject-level records carrying a birth date as text
`DOBTC` (a year, a year and month, or a full date), an
assessment moment `DTM`, and a reference start date `REFDT`.
All three reach the output as collected, blank when nothing was
collected.

**Variables:**

- `DOB_FIRST` holds the birth date with a missing month set to
  June and a missing day set to the first of the month; a full
  date stands as collected, and blank stays blank.
- `DOB_LAST` holds the birth date completed the same way, but
  with a missing day set to the last of the month, so February
  lands on the 28th or 29th by year; a full date stands as
  collected, and blank stays blank.
- `DOB_PREC_TX` holds `Y` for a year alone, `M` for a year and
  month, and `D` for a full date, read from the collected text;
  blank when the text is blank.
- `DOB_PREC_DT` holds the same code read from the completed
  first-day date; blank when there is no completed date.
- `DTMDT` holds the calendar day of the assessment moment;
  blank when the moment is blank.
- `DOBDY` holds the study day of the completed first-day date
  against the reference start, with the reference itself as
  day one and no zero; blank when either date is blank.

**Note:** a year alone may receive both a month and a day here,
while a complete date is never moved. Precision follows the
collected text through completion, and the study day counts a
collected day exactly as a supplied one would count.

**Standard:** ADaM | **Domain:** ADSL
