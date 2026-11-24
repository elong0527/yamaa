# Partial Start Dates With Placeholders

**Goal:** write an adverse event's collected start date and time as
ISO 8601 text that keeps every collected part, beside the text that
stops at the first part not collected, and read how much the kept
text carries.

**Input:** one specification, `spec.yaml`, over collected adverse
event records carrying the start year, month, day, hour and minute
as separate fields, any of which may be blank.

**Variables:**

- `AESTDTC` holds the start as ISO 8601 text up to its last
  collected part, with a hyphen standing for each part not collected
  before it, as SDTM writes a day collected in an unknown month
  (`2025---15`) or a minute collected at an unknown hour
  (`2025-03-15T-:30`); blank when no part was collected.
- `TRUNCDTC` holds the same start up to the first part not
  collected, so a day after an unknown month is left out; blank when
  the year was not collected.
- `STPREC` holds the precision read back from `AESTDTC`: `year`,
  `month`, `day` or `minute` for the finest collected part before the
  first part not collected, `none` when the year itself is unknown,
  and blank when no part was collected.

**Note:** the hyphens add parts without changing what the text
reads as: `STPREC` is the precision of `TRUNCDTC` whenever the year
was collected.
