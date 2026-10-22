# Reject Cleared Property

**Goal:** prepare subject-level records keyed by the subject
identifier (`USUBJID`).

**Input:** demographics records, each carrying the subject
identifier used to key the output.

**Variables:**

- `USUBJID` would be the subject identifier copied from the
  demographics record.

**Note:** the subject identifier takes its data type from a shared
parent file, and this file clears it. Every column must keep a data
type, so the run is rejected before any data is read and no
artifact is accepted.

## How to fix

Remove `columns.USUBJID.type` from `clear` to inherit the type
unchanged, or declare `USUBJID` with a complete valid type instead.
Only an optional field may be listed in `clear`.
