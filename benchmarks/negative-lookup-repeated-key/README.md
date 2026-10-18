# Reject a Matching Column Listed Twice

**Goal:** derive each subject's date of first exposure to treatment
(`TRTSDT`) from the subject's earliest exposure record.

**Input:** demographics records, and exposure records carrying the start
date of each administration (`EXSTDTC`).

**Variables:**

- `TRTSDT` would be the start date of the subject's earliest exposure
  record, and blank for a subject with no exposure record.

**Note:** exposure records are matched to a subject by listing the columns
the two must share, and the list names the subject identifier twice. A
repeated column states the same match again and may hide a column that was
meant instead, so the run is rejected before any data is read and no
artifact is accepted.

## How to fix

Name each matching column once. When the repeated entry was meant to be a
different column, name that column instead:

```yaml
intermediates:
  - id: FIRSTDOSE
    dataset: EX
    key: [STUDYID, USUBJID]
    order_by: [EX.EXSTDTC]
    keep: first
    no_match: null
```
