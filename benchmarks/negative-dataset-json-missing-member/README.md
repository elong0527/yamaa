# Reject a Dataset-JSON File That Does Not Count Its Records

**Goal:** build a subject listing carrying `SITEID` and `AGE` from
demographics received as a Dataset-JSON file.

**Input:** the received demographics `input/dm.json`, a Clinical Data
Interchange Standards Consortium (CDISC) Dataset-JSON 1.1 file that leaves
out `records`, the number of records the standard requires every file to
state.

**Variables:**

- `SITEID` would be the site the subject enrolled at, and `AGE` the age in
  whole years, each taken directly from the received demographics.

The count is how a reader knows that every record the sender wrote has
arrived. Without it the file is not a Dataset-JSON 1.1 file, so it is
rejected while it is read, before any record is used, and no artifact is
accepted.

## How to fix

Ask the sender for a file written to the Dataset-JSON 1.1 standard, which
states the number of records beside the dataset's name and label:

```json
"records": 2,
"name": "DM",
"label": "Demographics",
```

Do not add the count by hand unless you have confirmed, with the sender,
how many records the dataset holds.
