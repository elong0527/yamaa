# Reject a reference range stated twice

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/negative-mapping-from-duplicate-key.html)

**Goal:** carry sex and the numeric result into the output and
choose the upper limit of normal (`ANRHI`) from a reference table
by test and sex.

**Input:** collected laboratory records carrying test code
(`LBTESTCD`), sex (`SEX`), and numeric result (`LBSTRESN`), plus a
reference table carrying test code, sex, and upper limit
(`ANRHI`).

**Variables:**

- `ANRHI` would be the upper limit of normal for that test and
  sex, taken from the reference table row matching the collected
  test code and sex.

The reference table lists the same test (`ALT`) and sex (`F`) on
more than one line with different upper limits. Taking either
limit, or the first the file happens to list, would make the
result depend on file order rather than on the study's reference
ranges, so the run is rejected with no artifact accepted.

**Standard:** ADaM | **Domain:** ADLB

## How to fix

Make the lookup table unique on `[LBTESTCD, SEX]` by resolving the
conflicting `ALT/F` reference limits under the study's governed
reference-range rules. A lookup cannot choose one duplicate by file order. If
both rows are valid for different conditions, add the distinguishing field to
both the current-row source list and the lookup key list. For example, a
method-specific table would use matching lists such as
`source: [PARAMCD, SEX, METHOD]` and `key: [LBTESTCD, SEX, METHOD]`.
