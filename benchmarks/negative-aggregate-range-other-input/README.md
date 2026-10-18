# Reject Adverse Events Counted From Another Dataset's Dose Date

**Goal:** count each subject's treatment-emergent adverse events
(`TEAENUM`), the events that start on or after the first dose.

**Input:** demographics with one record per subject, exposure records
carrying each dose's start date (`EXSTDTC`), and adverse events carrying a
sequence number (`AESEQ`) and a start date (`AESTDTC`).

**Variables:**

- `TEAENUM` would be the number of the subject's adverse events that start
  on or after the first dose date, that day included, and blank when no
  event does.

**Note:** events are counted from a value of the subject row itself: a
field of the demographics record the row was built from, or a column built
for that row. The dose date is taken directly from the exposure records, a
different dataset, so it is not a value of the subject row. The run is
rejected before any data is read; no artifact is accepted.

## How to fix

Decide which dose starts the treatment-emergent period; usually it is the
subject's first dose. Bring that date onto each subject row as its own
column, the earliest exposure start date, declared before `TEAENUM`, and
count the events from that column:

```yaml
columns:
  - name: TRTSDT
    type: date
    label: Date of First Exposure to Treatment
    derivation:
      aggregate:
        expr: "MIN(EX.EXSTDTC)"
```

```yaml
between: {value: TRTSDT, upper: AESTDTC}
```

Leave `TRTSDT` out of the published columns when the artifact should not
carry it.
