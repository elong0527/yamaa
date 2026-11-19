# Dose Records with Additional Doses and the Dosing Interval

**Goal:** write one dose record of a population PK dataset for each
exposure record that covers repeated administrations of one dose: the
date/time of the first administration (`ADTM`), the dose (`AMT`), the
hours between administrations (`II`) and how many administrations follow
the first (`ADDL`).

**Input:** exposure (EX) records with the dose (`EXDOSE`), the dosing
frequency (`EXDOSFRQ`: `QD` once a day, `BID` twice a day, `ONCE` a single
dose) and the start and end of dosing (`EXSTDTC`, `EXENDTC`), each
collected with or without a time of day. A record with a dose above zero
gives one dose record, identified by its sequence number (`EXSEQ`).

**Variables:**

- `ADTM`: the first administration, the start as collected; a start with
  no time of day is taken at 00:00:00.
- `ATMF`: `H` when the time of day of `ADTM` was not collected; empty
  otherwise.
- `EVID`: 1, a dose record.
- `AMT`: the dose given at each administration.
- `II`: the hours between administrations; 0 when `ADDL` is 0, since a
  record of one dose has no interval.
- `ADDL`: the number of administrations after the first, one every
  interval of the frequency (24 hours for `QD`, 12 for `BID`) for as long
  as the next one is not after the end of dosing. An end with no time of
  day covers its whole day. 0 for a single dose and for a record with no
  end.

**Note:** the end of dosing only bounds the administrations and is not one
itself, so a record ending before the day's dose time has its last
administration the day before.
