# Reject a subject layout inherited from a parent file

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-adsl-inherited-output.html)

**Goal:** carry the subject identifier from demographics into
subject-level records, letting a parent file choose which
variables the final dataset would contain.

**Input:** demographics holding the subject identifier.

**Variables:**

The requested layout would carry the subject identifier from
demographics into the final dataset, but no dataset is produced:
the requested dataset must own that choice explicitly, so the run
is rejected before any data is read.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Declare the complete `output` in the entry file. Parent files may
provide a default for reuse, but the entry must replace it
explicitly:

```yaml
output:
  columns: [USUBJID]
```
