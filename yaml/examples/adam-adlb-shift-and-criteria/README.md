# ADaM ADLB: classify a result, its shift from baseline, and one criterion

This example uses a pre-derived ADLB slice and a `yamaa` specification to
derive one row per subject, parameter, and record:

- `AVAL` is the analysis value, and `ANRLO` and `ANRHI` the normal limits the
  parameter is read against. `ANRIND` places the value between them as `LOW`,
  `NORMAL`, or `HIGH`, and is empty when nothing was measured;
- `ABLFL` marks the record taken as the subject's baseline for the parameter.
  `BASE` repeats that record's value on every record of the parameter and
  `BNRIND` repeats its classification;
- `SHIFT1` reads as the baseline classification, then the one this record
  carries, so a result that stayed normal and one that moved out of range are
  told apart. It is empty when this record has no classification of its own;
- `R2BASE` is the analysis value as a multiple of the baseline value. A
  parameter whose baseline is zero has no ratio, since the multiple is not
  defined there;
- `CRIT1` states the criterion the record was assessed against, here a result
  more than three times the upper normal limit, and `CRIT1FL` says whether the
  record met it, `Y` or `N`. Both are empty on a record the criterion could not
  be assessed on, which differs from one assessed and not met.

Every classification, shift, and ratio on a record is read from the one record
the parameter's baseline flag marks, so a parameter with no marked baseline has
none of them.
