# Reject a Dataset-JSON Record With a Value Missing

**Goal:** build a subject listing carrying `SITEID` and `AGE` from
demographics received as a Dataset-JSON file.

**Input:** the received demographics `input/dm.ndjson`, a Clinical Data
Interchange Standards Consortium (CDISC) Dataset-JSON 1.1 file in its
newline-delimited form: the first line describes four columns, and each
later line is one record. The second subject's record holds three values.

**Variables:**

- `SITEID` would be the site the subject enrolled at, and `AGE` the age in
  whole years, each taken directly from the received demographics.

A record lists its values in column order, so a record with a value too few
does not say which column lost its value: reading the rest in order could
put a value under the wrong column. The file is rejected while it is read,
naming the record, and no artifact is accepted.

## How to fix

Ask the sender to write every record with one value per column, writing an
absent value as `null` rather than leaving it out:

```json
["PILOT7", "P7-742", "102", null]
```
