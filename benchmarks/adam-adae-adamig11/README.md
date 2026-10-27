# Analysis Start Time of an Adverse Event with ADaMIG 1.1

**Goal:** derive each adverse event's analysis start date and time
(`ASTDTM`), its date (`ASTDT`), its time of day (`ASTTM`) and the time
imputation flag (`ASTTMF`), and submit them in a SAS Transport version 5
file with the data-definition document.

**Input:** SDTM adverse event (AE) records with the collected start
(`AESTDTC`) as ISO 8601 text, collected to the second, the minute, the
hour, the day or the month.

**Variables:**

- `AETERM` is the adverse event as reported.
- `ASTDTM` is the start date and time: a start collected to the hour or
  the minute takes zero for the rest of its time, and a start collected as
  a date alone takes midnight; missing when the start was not collected to
  the day.
- `ASTDT` is the date part of `ASTDTM`; missing when `ASTDTM` is.
- `ASTTM` is the time of day of `ASTDTM` counted in seconds since
  midnight, so `08:30:15` is `30615`, and it is shown as `hh:mm:ss`;
  missing when `ASTDTM` is.
- `ASTTMF` says which part of the start time was imputed: `H` when the
  start was collected without a time, `M` when it stops at the hour and
  `S` when it stops at the minute; blank when the time was collected to
  the second or there is no start.

**Note:** the SAS Transport file stores `ASTTM` as an eight-byte numeric
value with the SAS format `TIME8.`, beside `ASTDT` with `DATE9.` and
`ASTDTM` with `DATETIME20.`. The data-definition document records these
formats. A time of zero flagged `H` is an imputed midnight rather than a
collected one.

The `project.yaml` guide declaration is ADaMIG 1.1; the data-definition
document records it and points to the SAS Transport version 5 dataset.
