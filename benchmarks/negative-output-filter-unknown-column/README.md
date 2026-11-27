# Reject a Kept-Rows Filter on an Input Field

**Goal:** publish the category B findings of each subject, numbered by test
within the subject (`SEQ`), with the subject (`SUBJ`), the test (`TEST`) and
its value (`VAL`).

**Input:** each input record is one finding carrying the subject, the
category (`CAT`), the test and its value.

**Variables:**

- `SEQ` would number a subject's findings from 1 in test order.

**Note:** the records to keep are chosen by the category of the input record,
but no variable of the requested dataset carries the category, so a completed
record has no category to be kept by. The run is rejected before any data is
read and no artifact is accepted.

## How to fix

Declare the category as a variable, internal when the published part should
not carry it, and keep the records by that variable:

```yaml
rows:
  - id: all_rows
    dataset: SRC
    derivations:
      SUBJ: SRC.SUBJ
      CAT: SRC.CAT
      TEST: SRC.TEST
      VAL: SRC.VAL

columns:
  # ... the existing columns, then:
  - {name: CAT, type: str}

output:
  path: obs_b.csv
  columns: [SUBJ, SEQ, TEST, VAL]
  filter: CAT = 'B'
```

When the numbers should count only the kept records, keep them with the row
template's own `filter` instead, which reads the input record before the
numbers are assigned.
