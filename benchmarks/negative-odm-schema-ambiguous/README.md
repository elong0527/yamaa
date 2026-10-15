# Reject Two Fields That Name One Value

**Goal:** derive `AGE` for one record per subject from the subject's
collected answer.

**Input:** long-form Operational Data Model (ODM) data with one row per
collected item. The export carries the stored value twice, as `Value` and
as `VALUE`, and field names match whatever their case.

**Variables:**

- `AGE` would be the subject's collected age.

Both fields name the stored value, and nothing says which one holds it, so
the input is rejected before any record is read.

## How to fix

Keep one field for the stored value. If the second is a vendor's own copy,
drop it from the export or rename it to a name that is not an ODM field,
such as `VALUE_RAW`.
