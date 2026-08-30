# ADaM ADSL: reconcile randomization strata

This example uses demographics, disease history, and the strata recorded at
randomization to prepare one record per subject:

- `METSTATR`, `ECOG0R`, and `REGIONUSR` are the three values recorded for
  randomization;
- `METSTAT`, `ECOG0`, and `REGIONUS` are the corresponding values obtained
  independently from the entry data;
- `STRAT1R` combines the randomization values, while `STRAT1` combines the
  independently obtained values in the same order.

The two combined values must agree. A disagreement is reported against the
subject rather than resolved in favour of either source. The expected CSV
records the completed rows presented to that check, but the disagreement still
rejects the run and no artifact is accepted from this input.

## How to fix

Query the disagreement and correct whichever source is wrong. If both values
are valid but serve different purposes, document which one governs the analysis
and replace the equality check with that reconciliation policy. Do not silently
prefer the randomization value or the independently collected value.
