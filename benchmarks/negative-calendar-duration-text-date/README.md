# Reject a calendar duration from a start date held as text

**Goal:** write how long before the first dose each adverse event began as
ISO 8601 duration text in years, months and days (`AEPREDUR`).

**Input:** adverse event records, one per event (`AESEQ`), with the event's
start date as the case report form collected it and the subject's date of
first dose.

**Variables:**

- `AEPREDUR` would hold the calendar time from the event's start to the
  first dose, so 26 August 2007 to 27 September 2007 gives `P1M1D`. Only two
  dates give a calendar duration, and the collected start is still text that
  nothing has read as a date, so the run is rejected before any data is read
  and no artifact is accepted.

## How to fix

Declare the start a date where the input is read, so each value is checked
as a complete date as the records are read:

```yaml
input:
  AE_RAW:
    path: input/ae_raw.csv
    types: {AESEQ: int, AESTDTC: date, RFXSTDTC: date}
```

When a start may be collected to the month or the year only, keep it as
text, take its date in a column of its own with `to_date`, and name that
column as the `start`; a partial start then has no date and no duration:

```yaml
- name: AESTDT
  type: date
  label: Start Date of Adverse Event
  derivation:
    to_date: {source: AE_RAW.AESTDTC}
```
