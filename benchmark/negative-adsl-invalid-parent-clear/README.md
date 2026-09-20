# Reject removal of a required variable property

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-adsl-invalid-parent-clear.html) [![Lifecycle: reviewed](https://img.shields.io/badge/Lifecycle-reviewed-yellow)](https://github.com/elong0527/yamaa/blob/main/benchmark/README.md#lifecycle)

**Goal:** prepare subject-level records keyed by subject
identifier, with the identifier copied from the matching
demographics record.

**Input:** demographics records, each carrying the subject
identifier used to key the output.

**Variables:**

- The subject identifier copied from the matching demographics
  record. A required property the result must keep was cleared,
  so the run is rejected before any data is read and no record
  is produced.

**Standard:** ADaM | **Domain:** ADSL

## How to fix

Omit `type` to inherit it unchanged, or replace it with a
complete valid value. Only optional immediate fields may use
`null` to clear an inherited value.
