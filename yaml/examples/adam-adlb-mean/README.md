# ADaM ADLB: calculate each subject's mean result

This example uses collected neutrophil results with a `yamaa` specification to
derive one row per result:

- `AVAL` is the collected result;
- `AVALMEAN` is the arithmetic mean of the subject's non-missing results for
  the parameter, carried onto each of those result records.

Missing results do not contribute to the mean. A subject with records but no
collected result has a missing mean rather than a measured zero.
