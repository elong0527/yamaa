# Reject a Record Without Its Subject

**Goal:** derive `AGE` for one record per subject from the subject's
collected answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item. One record of the export has lost its subject.

**Variables:**

- `AGE` would be the subject's collected age.

A record without its subject cannot be placed, so the input is rejected
before any record is read, naming the field and the first record that
lacks it.

## How to fix

Restore the subject of that record in the export, or remove the record if
it belongs to no subject. Every record carries its study, metadata version,
subject, visit, form, item group, and item; only the repeat keys and the
value may be missing.
