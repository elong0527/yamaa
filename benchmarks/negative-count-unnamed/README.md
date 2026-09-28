# Fail an Unnamed Count on Bad Data

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-count-unnamed.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

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

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Correct the baseline flags in the incoming records so each subject and
parameter has exactly one record flagged `Y`.
