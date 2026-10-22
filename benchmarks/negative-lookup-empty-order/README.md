# Reject a Last Record Chosen from No Order

**Goal:** derive each subject's date of last exposure to treatment
(`TRTEDT`) from the subject's last exposure record.

**Input:** demographics records, and exposure records carrying the end
date of each administration (`EXENDTC`).

**Variables:**

- `TRTEDT` would be the end date of the subject's last exposure record, and
  blank for a subject with no exposure record.

**Note:** the last record is asked for, but the order that would say which
record comes last lists nothing, so a subject with several administrations
has no last record. The run is rejected before any data is read and no
artifact is accepted.

## How to fix

State the order that makes one administration the last, here the latest end
date:

```yaml
intermediates:
  - id: LASTDOSE
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXENDTC]
    keep: last
```

Add a tie-breaking field, such as a sequence number, when two
administrations can end on the same day.
