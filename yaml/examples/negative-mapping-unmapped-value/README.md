# ADaM ADRS: reject an unmapped response

This example uses one overall-response assessment to attempt one analysis
record:

- `AVALC` is meant to translate the collected response into its analysis
  category.

The collected value `NE` has no dictionary entry and no fallback is declared.
Treating it as missing or choosing another response would conceal an incomplete
rule, so the run must fail and no artifact is accepted.
