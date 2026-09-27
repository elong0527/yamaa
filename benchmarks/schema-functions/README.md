# Call a Project Routine

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-functions.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive an adjusted ratio (`RATIO`) and its variants by calling
the project's own ratio routine, showing how the call fills in
defaults, handles missing values, and switches the result to a
percentage.

**Input:** a test file with five records carrying a numerator (`NUM`),
a denominator (`DEN`), and an optional adjustment (`ADJ`) that is blank
for some records.

**Variables:**

- `ID`: the test record identifier, carried through unchanged.
- `RATIO`: the numerator divided by the denominator, rounded to two
  places; empty when the numerator is missing.
- `RATIO_DEFAULT`: the same value as `RATIO`: leaving the rounding
  places out of the call uses the project's own default of two.
- `RATIO_PCT`: the same ratio as a percentage, rounded to one place.
- `RATIO_ADJ_MISSING`: `RATIO` plus the adjustment; a blank adjustment
  counts as zero.
- `RATIO_MISSING`: always empty: the call is never made when the
  numerator is missing, so no result exists.

**Note:** the project's routine is the same for every call; only the
arguments differ. A missing value the routine does not accept skips the
call entirely, while a missing value it accepts arrives as missing and
counts as zero.

**Standard:** TEST | **Domain:** TEST
