# ADaM ADLB: reject a log result from an undetectable value

This example uses collected viral-load results to attempt one record per
subject and parameter:

- `AVAL` is the collected number of copies per millilitre;
- `AVALLN` is its natural logarithm, which the analysis models rather than the
  untransformed result.

One result was reported as zero because the assay detected nothing, and zero
has no logarithm. A result below the limit of detection needs a stated
substitution before it can be transformed, so the run must fail and no artifact
is accepted.
