# Reject a Dataset-JSON Record With a Value Missing

**Goal:** build a subject listing carrying `SITEID` and `AGE` from
demographics received as a Dataset-JSON file.

**Input:** the received demographics `input/dm.ndjson`, a Clinical Data
Interchange Standards Consortium (CDISC) Dataset-JSON 1.1 file in its
newline-delimited form: the first line describes the columns, and each
later line is one record. A record holds fewer values than the file
declares.

**Variables:**

- `SITEID` would identify the site where the subject enrolled. The rejected
  file produces no output value.
- `AGE` would give the subject's age in whole years. The rejected file
  produces no output value.

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
