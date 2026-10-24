# Refuse arithmetic over a list of sequence numbers

**Goal:** build one subject-level record per subject carrying `NEXTSEQ`, the
next free adverse event sequence number: one more than the highest the
subject has.

**Input:** demographics with one record per subject, and collected adverse
events with their sequence numbers.

**Variables:**

- `NEXTSEQ` would be one more than the subject's highest adverse event
  sequence number.

**Note:** the request adds one to the subject's sequence numbers joined into
one text. A list written as text is not a number, and has no single value to
add one to, so the run is rejected before any data is read.

## How to fix

Take the highest sequence number, not the list, and add one to it:

```yaml
- name: NEXTSEQ
  type: int
  derivation:
    aggregate:
      key: [STUDYID, USUBJID]
      expr: "MAX(AE.AESEQ) + 1"
```

The order and the separator that the list needed go with it: the highest
value reads neither.
