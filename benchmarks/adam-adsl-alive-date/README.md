# Derive the Last Known Alive Date and Death Date from Multiple Domains

**Goal:** derive `LSTALVDT`, `DTHFL`, `DTHDT`, and `DTHDTF` for each
subject from the dates collected across the study's exposure, adverse
event, disposition, visit, vital signs, and survival follow-up records.

**Input:** demographics (`DM`) carrying the collected death date
(`DTHDTC`) and death flag (`DTHFL`), alongside Study Data Tabulation
Model (SDTM) records whose collected dates can show a subject alive:
exposure (`EX`), adverse events (`AE`), disposition (`DS`), subject
visits (`SV`), vital signs (`VS`), and survival follow-up in subject
status (`SS`).

**Variables:**

- `LSTALVDT` is the latest date on which a record shows the subject
  alive: an exposure start or end (`EXSTDTC`, `EXENDTC`), an adverse
  event start or end (`AESTDTC`, `AEENDTC`), a disposition date
  (`DSSTDTC`) except on a `DEATH` or `LOST TO FOLLOW-UP` record, a visit
  start (`SVSTDTC`), a vital signs date (`VSDTC`), or a survival
  follow-up date (`SSDTC`) whose status (`SSSTRESC`) is `ALIVE`. It never
  falls after `DTHDT`: a record dated after death is a data error, and
  the date stops at the death date. Empty when no such record has a date.
- `DTHFL` is `Y` when demographics records the subject as dead, a death
  without a collected date included; empty otherwise.
- `DTHDT` is the collected death date completed to a day. A year and
  month take the first day of that month and a year alone 1 January,
  unless the latest of the dates counted for `LSTALVDT` falls on a later
  day inside the collected month or year: then the death date is that
  day. A complete death date stands as collected. Empty when no death
  date was collected.
- `DTHDTF` is `D` when the death date's day was supplied and `M` when
  its month and day were; empty for a complete or missing death date.

**Note:** every collected date counts on the day it names. A date with a
time keeps its date, a year and month count as the first of that month,
and a year alone as 1 January, so completing a partial date never claims
contact beyond what was collected and a complete date within that month
or year wins. The domains are this study's: another study reads its own
(concomitant medications or laboratory samples, for example) under the
same rule.
