# Death Date Flag

**Goal:** derive date/time of death (`DTHDTC`) and death flag (`DTHFL`) for
each demographics subject.

**Input:** demographics subjects, disposition records with status and start
date, and adverse events with outcome and end date.

**Variables:**

- `DTHDTC` is the latest disposition death date, or the latest fatal adverse
  event end date when disposition has no death; empty when neither source
  records a death.
- `DTHFL` is `Y` when either source supplies a death date; empty otherwise.

**Note:** when the sources record different death dates, the disposition date
wins. An undated death or fatal event cannot displace a dated one.
