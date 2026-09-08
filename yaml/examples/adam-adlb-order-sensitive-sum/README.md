# ADaM ADLB: sum floating-point values in source record order

Collected laboratory results produce one analysis record per collected result:

- `AVAL` is the collected analysis value;
- `AVALSUM` is the subject's values added in their stored record order and
  carried onto every record for that subject.

Binary64 addition makes the total sensitive to that order: adding `0.1`,
`0.2`, and `0.3` produces `0.6000000000000001`, while adding the same values
in reverse produces `0.6`. The CSV files are reviewable illustrations;
production data should use Parquet and need not duplicate it as CSV.
