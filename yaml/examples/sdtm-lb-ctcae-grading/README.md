# SDTM LB: assign toxicity grades

This example uses haematology results with sex and a reproduced toxicity grade
to derive one record per result:

- `LBTESTCD` and `LBSTRESN` identify the laboratory test and its standardized
  numeric result;
- `ATOXGR` is the toxicity grade implied by the test's thresholds. Neutrophils
  use one set of concentration bands, while haemoglobin uses different normal
  limits for male and female subjects;
- `SEX` is retained so the haemoglobin threshold applied to a record is
  visible.

The independently supplied grade must equal the reproduced grade. Each test
and sex combination owns its threshold set, so adding a combination requires
adding its bands explicitly.
