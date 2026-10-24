Following CDISC ADaM standards, use the provided PC and PP datasets to create
an ADPP dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, AVALU

- Each subject has two records: PARAMCD "MRCMAXGM" with PARAM "Geometric
  Mean Metabolite Ratio Cmax" and AVALU "RATIO", and PARAMCD "C2HREPSD"
  with PARAM "Replicate SD of 2 H Concentration" and AVALU "ng/mL".
- For MRCMAXGM, AVAL is the geometric mean of the subject's PPSTRESN values
  with PPTESTCD "MRCMAX" that are above zero: the exponential of the mean
  of their natural logarithms. A value of zero or no value is left out.
- For C2HREPSD, AVAL is the sample standard deviation, with divisor one
  less than the count, of the subject's PCSTRESN values with PCTPT "2H",
  one for each replicate assay. AVAL has no value when the subject has only
  one such value.
- AVAL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
