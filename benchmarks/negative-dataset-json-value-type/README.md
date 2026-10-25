# Reject a Dataset-JSON Value Against Its Column's Type

**Goal:** build a subject listing carrying `SITEID` and `AGE` from
demographics received as a Dataset-JSON file.

**Input:** the received demographics `input/dm.json`, a Clinical Data
Interchange Standards Consortium (CDISC) Dataset-JSON 1.1 file whose `AGE`
column states that it holds integers, while the second subject's age is
written as 67.5.

**Variables:**

- `SITEID` would be the site the subject enrolled at, and `AGE` the age in
  whole years, each taken directly from the received demographics.

The column's type is part of the dataset: a value that is not a whole number
in a column of integers is a defect in the data, not a reason to round it or
to read the whole column as decimals. The file is rejected while it is read,
naming the column, the record and the value, and no artifact is accepted.

## How to fix

Confirm the subject's age with the sender and have the file corrected at
its source. If the study really collects a fractional age, the sender
states the column as a floating-point or decimal column, and the study
reads it as one:

```json
{"itemOID": "IT.DM.AGE", "name": "AGE", "label": "Age", "dataType": "float"}
```
