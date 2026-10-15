# Fail an Unnamed Count on Bad Data

**Goal:** carry the analysis date (`ADT`), the numeric result (`AVAL`),
and the baseline flag (`ABLFL`) for each laboratory result, and check that
each subject and parameter has exactly one flagged baseline result.

**Input:** collected laboratory results carrying analysis date
(`ADT`), numeric result (`AVAL`), and baseline flag (`ABLFL`). One subject
has two flagged records and another has none.

**Variables:**

- `ABLFL` is the input baseline flag: `Y` on the record that serves as the
  baseline for the subject and parameter, blank on every other record.

**Note:** the grouped count has no `id`, and the declaration is valid. It
runs and reports the failing subject-parameter groups at its specification
path. The bounds remain available in the declaration at that path.

## How to fix

Correct the baseline flags in the incoming records so each subject and
parameter has exactly one record flagged `Y`.
