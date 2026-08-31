# ADaM ADEX: expand an aggregate dose into its administrations

This example uses collected exposure records to derive one row per
administration:

- `EXSEQ` identifies the collected record an administration came from, and
  `EXTRT`, `EXDOSE`, and `EXDOSU` are the treatment given, the amount given at
  each administration, and its unit;
- `ASTDT` and `AENDT` are the first and last day the collected record covers,
  repeated on every administration taken from it;
- `NDOSE` is how many administrations the record stands for, and `ADOSEN`
  numbers them from one through that count.

A record with a count of zero contributes no administration row. Every other
record contributes exactly the number it declares, without a fixed maximum in
the specification.
