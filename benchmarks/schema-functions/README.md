# Call a Project Routine

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-functions.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

**Goal:** derive an adjusted ratio (`RATIO`) and its variants by calling
the project's own ratio routine, showing how the call fills in
defaults, handles missing values, and switches the result to a
percentage.

**Input:** `spec.yaml` reads a small test file (`input/source.csv`)
carrying a numerator (`NUM`), a denominator (`DEN`), and an optional
adjustment (`ADJ`), and calls the ratio routine from the benchmark's
own Python project root (`python/`); the expected result is
`expected/test.csv`.

**Variables:**

- `ID`: the test record identifier, carried through unchanged.
- `RATIO`: the numerator divided by the denominator, rounded to two
  places; empty when the numerator or the denominator is blank.
- `RATIO_DEFAULT`: the same value as `RATIO`: leaving the rounding
  places out of the call uses the project's own default of two.
- `RATIO_PCT`: the same ratio as a percentage, rounded to one place.
- `RATIO_ADJ_MISSING`: `RATIO` plus the adjustment; a blank adjustment
  counts as zero.
- `RATIO_MISSING`: empty on every record: the call passes no numerator
  at all, so the routine is never invoked.

**Note:** a blank value the routine does not accept skips the call
entirely, so that record has no result; a blank value it accepts
arrives as missing and counts as zero.

**Standard:** TEST | **Domain:** TEST
