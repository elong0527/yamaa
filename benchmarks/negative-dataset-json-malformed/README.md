# Reject a Dataset-JSON File Cut Off in Transfer

**Goal:** build a subject listing carrying `SITEID` and `AGE` from
demographics received as a Dataset-JSON file.

**Input:** the received demographics `input/dm.json`, a Clinical Data
Interchange Standards Consortium (CDISC) Dataset-JSON 1.1 file whose
transfer stopped part-way through its last record, so the file ends in the
middle of it.

**Variables:**

- `SITEID` would be the site the subject enrolled at, and `AGE` the age in
  whole years, each taken directly from the received demographics.

A file that stops before its end is not a complete dataset: the records it
was cut from are unknown, and reading the part that arrived could quietly
drop subjects. The file is rejected while it is read, before any record is
used, and no artifact is accepted.

## How to fix

Transfer the file again and confirm it arrived whole, for example by
comparing its size with the sender's. Do not repair the end by hand: the
missing values are not in the file to restore.
