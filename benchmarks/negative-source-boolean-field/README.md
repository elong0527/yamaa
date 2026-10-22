# Reject a True-or-False Field in a Parquet Export

**Goal:** carry the randomized population flag (`RANDFL`) onto a subject
listing.

**Input:** a Parquet export from the randomization system with one record
per subject, in which `RANDFL` is stored as a true-or-false (Boolean) value
rather than as text.

**Variables:**

- `RANDFL` would be `Y` for a subject in the randomized population, taken
  from the export.

A true-or-false value is none of the kinds of value a study column holds:
text, a whole number, a number, a date or a date and time. Reading it as
text, as `true` or `1`, or as a number would each be a choice the study
never made, and none of them is the `Y` the flag takes. The run is rejected
while the export is read, and no artifact is accepted.

## How to fix

Have the randomization system export the flag the way the study defines it:
the text `Y` for a randomized subject and an empty value for one who was not.
The study does not need to change.

```yaml
input:
  RND: input/rand.parquet
```
