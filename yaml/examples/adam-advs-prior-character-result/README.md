# ADaM ADVS: retain the latest earlier character result

This example uses character results with incomplete series and visit values to
derive one analysis record per collected row:

- `SERIES` identifies an analysis series; rows with no series value remain in
  one shared series;
- `AVISITN` orders the rows, with missing visits after numbered visits and
  collection order settling equal visits;
- `AVALC` is the current character result and can be empty;
- `PREVAVALC` is the closest earlier non-empty result in the same series. It is
  empty at the start of a series and ignores the current result.

The result retains character values unchanged and can cross consecutive empty
results.
