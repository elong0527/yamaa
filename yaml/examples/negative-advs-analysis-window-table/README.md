# ADaM ADVS: reject an analysis window read from the study's window table

This example uses a pre-derived ADVS slice and the study's analysis window
table to attempt one row per record:

- `ADT`, `ADY`, and `AVAL` are the record's analysis date, its study day, and
  the value measured, all carried through as given;
- `AVISIT` and `AVISITN` are the analysis window the record falls in, and its
  order. The table states each window's first and last study day, so a record
  belongs to the one window whose range contains its day, and a record with no
  study day belongs to none;
- `AWTARGET` is the day the protocol schedules that window on, and `AWTDIFF`
  how far the record sits from it, negative before the target and positive
  after;
- `ANL01FL` marks the record that represents its subject in the window: the
  one closest to the target day, and the lower sequence number when two are
  equally close.

Which window a record belongs to depends on that record's own study day, and a
value carried on a record cannot narrow the table rows the record is matched
against. Every other way of reaching the table matches on equal values, and a
day range is not one, so the run must fail and no artifact is accepted. The
expected output records the intended result.

## How to fix

First decide where the study's windows are allowed to live. If the
specification may carry them, state the boundaries as a list of days and
classify the study day directly, which is what
[`adam-advs-analysis-visit`](../adam-advs-analysis-visit/) does:

```yaml
derivation:
  cut:
    source: ADY
    breaks: [0, 2, 22, 43]
    labels: [SCREENING, BASELINE, WEEK 2, WEEK 4, POST-TREATMENT]
```

The boundaries are then part of the specification and change with every study,
and no implementation can check them against the study's own window table.

If the table must stay the one place the windows are stated, resolve each
record's window before this specification reads it and carry `AVISIT`,
`AVISITN`, and `AWTARGET` on the input slice. Do not pick one table row by
ordering the whole table: it answers with a window whose range the record's
day may not be in at all.
