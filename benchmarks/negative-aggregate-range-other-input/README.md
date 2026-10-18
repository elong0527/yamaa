# Reject an Event Window Opened by Another Dataset's Date

**Goal:** count each subject's treatment-emergent adverse events (`NTEAE`),
next to the date the subject first received study treatment (`TRTSDT`).

**Input:** demographics carrying one record per subject, a first-exposure
table carrying each subject's first treatment date (`TRTSDT`), and adverse
events carrying each event's start date (`AESTDT`).

**Variables:**

- `TRTSDT` would be the subject's first treatment date from the
  first-exposure table, matched to the subject by study and subject.
- `NTEAE` would be the number of the subject's adverse events that start on
  or after the first treatment date, counting an event that starts on that
  date. An event with no start date is not counted. It would be blank when
  the subject has no first treatment date or no event in that window.

**Note:** an event window is opened by a value of the subject row itself: a
field of the demographics record the row was built from, or a column built
for that row. The first treatment date is taken directly from the
first-exposure table, a different dataset, so it is not a value of the
subject row. The run is rejected before any data is read; no artifact is
accepted.

## How to fix

Decide which date opens the window; usually it is the first treatment date
recorded for the subject. The subject row already carries that date as its
own column, `TRTSDT`, matched to the subject by study and subject. Open the
window on that column:

```yaml
between: {value: TRTSDT, upper: AESTDT}
```

When the row does not carry the date yet, add it as a column first, for
example `TRTSDT` derived from `EXF.TRTSDT`, and leave it out of the
published columns when the artifact should not carry it.
