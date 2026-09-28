# Reject an Unnamed Count

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/negative-count-unnamed.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** carry the analysis date (`ADT`), the numeric result (`AVAL`),
and the baseline flag (`ABLFL`) for each laboratory result, and check that
each subject and parameter has exactly one flagged baseline result.

**Input:** collected laboratory results carrying analysis date
(`ADT`), numeric result (`AVAL`), and baseline flag (`ABLFL`). One subject
carries two `Y` flags for a parameter; another subject carries none.

**Variables:**

- `ABLFL` is the input baseline flag: `Y` on the record that serves as the
  baseline for the subject and parameter, blank on every other record.

**Note:** the baseline-count rule carries no name, and that is fine. The
check still runs, and its failure report still names the offending
subject-parameter combinations with their observed counts; the bounds the
data broke live with the rule's definition. The expected output records the
completed rows presented to that check.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Correct the flag in the incoming records so that one record carries
it, choosing the record the study's baseline definition selects,
ordinarily the latest result on or before the first exposure.

Do not widen the count to accept two records; a second baseline is
a defect in the data rather than a policy the analysis can adopt.
