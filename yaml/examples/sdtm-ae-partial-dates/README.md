# Keep partial dates at collected precision

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/sdtm-ae-partial-dates.html)

**Goal:** build one record per collected adverse event, keeping
partial start and end dates at collected precision and deriving
study day only for complete dates: `AETERM`, `AESTDTC`, `AEENDTC`,
`AESTDY`, and `AEENDY`.

**Input:** collected adverse-event rows with separate year, month,
and day fields for each start and end date, plus demographics rows
carrying the reference start date. An empty month or day means that
component was unknown.

**Variables:**

- `AETERM` is the event term as reported.
- `AESTDTC` keeps the collected start date at day, month, or year
  precision.
- `AEENDTC` keeps the collected end date at day, month, or year
  precision.
- `AESTDY` is the start study day when a complete start date was
  collected; missing for month- and year-precision starts.
- `AEENDY` is the end study day when a complete end date was
  collected; missing for month- and year-precision ends.

**Note:** no unknown date component is replaced. A month-precision
value such as `2026-04` and a year-precision value such as `2025`
remain exactly that precise in the record, while their study-day
fields stay missing.

**Standard:** SDTM | **Domain:** AE
