# Calculate each subject's mean result

**Goal:** add the subject's mean result (`AVALMEAN`) for each
laboratory parameter to every collected record.

**Input:** collected laboratory (LB) records carrying study,
subject, sequence number, test code, and the collected numeric
result.

**Variables:**

- `AVAL`: the collected numeric result for the record; empty
  when no usable result was collected.
- `AVALMEAN`: the mean of the subject's non-missing `AVAL`
  values for the parameter, repeated on every record for that
  subject and parameter, including a record whose own `AVAL`
  is empty; empty, never zero, when the subject has no
  non-missing `AVAL` for the parameter.

**Note:** the mean is taken separately for each test code, so
each of a subject's records carries the mean for its own test.
