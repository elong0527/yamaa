# Reject an Event Numbering That Groups by the Subject Twice

**Goal:** number each subject's adverse events in `AOCCSEQ`, earliest first.

**Input:** adverse event records carrying the analysis start date (`ASTDT`)
and the event's sequence number (`AESEQ`).

**Variables:**

- `ASTDT` would be the event's analysis start date.
- `AOCCSEQ` would number each subject's events from 1 in order of their
  start date, the event's sequence number breaking same-day ties.

**Note:** the events are numbered separately for each study and subject, and
the list that says so names the subject twice. A repeated entry states the
same grouping again and may hide a variable that was meant instead, so the
run is rejected before any data is read and no artifact is accepted.

## How to fix

Name each grouping variable once. When the repeated entry was meant to be a
different variable, name that variable instead:

```yaml
- name: AOCCSEQ
  type: int
  derivation:
    row_number:
      window:
        group_by: [STUDYID, USUBJID]
        order_by: [ASTDT, AESEQ]
```
