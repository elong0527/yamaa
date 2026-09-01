# ADaM ADEX: derive administration-level exposure records

This example uses administration-level exposure data to derive one analysis
row per recorded administration:

- `EXSEQ` identifies the source exposure interval;
- `EXTRT`, `EXDOSE`, and `EXDOSU` are the treatment, amount, and unit recorded
  for each administration;
- `ASTDT` and `AENDT` are the first and last day of the source interval;
- `ADOSEN` orders administrations within that interval, and `NDOSE` is the
  total number of administrations it represents.

Every administration already appears as an input record. An aggregate exposure
record must be normalized to this grain upstream before it is read here.
