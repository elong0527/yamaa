# Reject a Dataset-JSON 1.0 File

**Goal:** build a subject listing carrying `SITEID` from demographics
received as a Dataset-JSON file.

**Input:** the received demographics `input/dm.json`, a Clinical Data
Interchange Standards Consortium (CDISC) Dataset-JSON file that names
version 1.0.0, holding `STUDYID`, `USUBJID` and `SITEID`.

**Variables:**

- `SITEID` would be the site the subject enrolled at, taken directly from
  the received demographics.

Only Dataset-JSON 1.1 files are read. A 1.0 file nests its records and their
types in another structure, so reading it as 1.1 could give a value a type
the sender never gave it. The file is rejected while it is read, before any
record is used, and no artifact is accepted.

## How to fix

Ask the sender for the dataset as Dataset-JSON 1.1, and read that file. Its
first members name the version it follows:

```json
{"datasetJSONCreationDateTime": "2026-09-30T10:15:00",
 "datasetJSONVersion": "1.1.0", ...}
```

Do not edit the version of a 1.0 file by hand; the version is a property of
how the file was written, not a label. A file converted to 1.1, or to CSV, by
a tool outside the study is read under the name of what it then holds.
