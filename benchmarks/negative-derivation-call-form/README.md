# Reject a Date Conversion Written as Text

**Goal:** record `BRTHDT`, each subject's date of birth, from the
collected birth date and time.

**Input:** collected demographics with the study and subject
identifiers and the collected birth date, with or without a time
(`BRTHDTC`).

**Variables:**

- `BRTHDT` would be the calendar date of birth, taken from the date part
  of the collected text.

**Note:** a plain string in this position always names a value to copy
and is never read as an instruction. Written like a function call
around the collected value, it names no value at all, since parentheses
are not part of any name, so the run is rejected before any record is
read.

## How to fix

Write the conversion as the operation it names, with the collected value
as its source:

```yaml
derivation:
  to_date:
    source: DM.BRTHDTC
```

A plain string stays the right spelling for copying a collected value
unchanged, such as `derivation: DM.BRTHDTC` for a text column.
