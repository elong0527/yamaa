# ADaM ADLB: calculate each subject's mean result

[![Dashboard](https://img.shields.io/badge/Dashboard-view-0c5e4b)](https://elong0527.github.io/yamaa/examples/adam-adlb-mean.html)

This example uses collected neutrophil results with a `yamaa` specification to
derive one row per result:

- `AVAL` is the collected result;
- `AVALMEAN` is the arithmetic mean of the subject's non-missing results for
  the parameter, carried onto each of those result records.

Missing results do not contribute to the mean. A subject with records but no
collected result has a missing mean rather than a measured zero.
