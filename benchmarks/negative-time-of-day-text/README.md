# Reject a Time of Day Taken from Text

**Goal:** count each adverse event's analysis start time (`ASTTM`) in
seconds since midnight.

**Input:** adverse event records, each identified by study, subject and
sequence, carrying the reported term and the collected start as ISO 8601
text (`AESTDTC`).

**Variables:**

- `ASTTM` would hold the start's time of day in seconds since midnight,
  so `08:30:15` gives `30615`. Only a date and time has a time of day,
  and the start is still text that nothing has read as one, so the run is
  rejected before any data is read and no artifact is accepted.

## How to fix

Complete the collected start into a date and time first, deciding what a
start collected without its time or without its seconds becomes, and
take the time of day from that column:

```yaml
- name: ASTDTM
  type: datetime
  derivation:
    impute:
      type: datetime
      source: AE.AESTDTC
      time: first

- name: ASTTM
  type: int
  derivation:
    time_of_day:
      source: ASTDTM
```

Pair the time with the imputation flag of `ASTDTM`, so a start whose time
was supplied is not read as a collected midnight.
