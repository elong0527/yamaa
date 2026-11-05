# A Time of Day as a Column Type

**Goal:** derive each adverse event's analysis start date and time
(`ASTDTM`), its time of day as a time value (`ASTTM`), the time imputation
flag (`ASTTMF`) and a flag for a start in the afternoon (`ASTPMFL`), and
submit them in a Dataset-JSON file with the data-definition document.

**Input:** `spec.yaml`, one ADaM adverse event specification that
`project.yaml` submits in Dataset-JSON with its data-definition document,
reading SDTM adverse event (AE) records with the collected start
(`AESTDTC`) as ISO 8601 text, collected to the second, the minute, the hour,
the day or the month.

**Variables:**

- `AETERM` is the adverse event as reported.
- `ASTDTM` is the start date and time: a start collected to the hour or
  the minute takes zero for the rest of its time, and a start collected as
  a date alone takes midnight; missing when the start was not collected to
  the day.
- `ASTTM` is the time of day of `ASTDTM`, a time written `hh:mm:ss`, so
  `2025-03-04T08:30:15` gives `08:30:15`; missing when `ASTDTM` is.
- `ASTTMF` says which part of the start time was imputed: `H` when the
  start was collected without a time, `M` when it stops at the hour and
  `S` when it stops at the minute; blank when the time was collected to
  the second or there is no start.
- `ASTPMFL` is `Y` when `ASTTM` is at or after `12:00:00`, and blank
  otherwise, including when there is no start time.

**Note:** a `time` column holds a time of day with no date and no zone,
written `hh:mm:ss`. `to_time` takes it from a date and time, and it compares
only with another time, such as the literal `TIME '12:00'`. In the
Dataset-JSON file `ASTTM` keeps its `hh:mm:ss` text with `dataType` `time`
and asks for the integer the data-definition document describes: an
eight-byte integer of seconds since midnight shown with the SAS format
`TIME8.`, beside `ASTDTM` with `DATETIME20.`.
