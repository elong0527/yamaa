# Select a Measurement Closest to Each Reference Day

[![Dashboard](https://img.shields.io/badge/Dashboard-view-1f3a5c)](https://elong0527.github.io/yamaa/benchmark/schema-lookup-intermediate-correlated-derive.html)
[![Lifecycle: draft](https://img.shields.io/badge/Lifecycle-draft-lightgrey)](https://github.com/elong0527/yamaa/blob/main/benchmarks/README.md#lifecycle)

Each subject and reference day receives the measurement nearest that day.
`BASE` supplies the subject identifier `ID` and reference day `REF`;
`SRC` supplies candidate measurement days and their text values.

`VALUE` is the selected measurement, and `DIST` is the absolute difference
between its day and `REF`. For `S1`, reference day 10 selects `b` on day 11,
while reference day 3 selects `a` on day 2 from the same candidates.
For `S2`, days 9 and 11 are equally close to reference day 10; the first
record selects `c`. A candidate with no day is excluded. `S4` has no
candidate, so both selected values are missing. The unmatched `S3`
candidate contributes no output row.
