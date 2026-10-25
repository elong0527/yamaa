Following CDISC ADaM standards, use the provided PC and PP datasets to create
an ADPP dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, AVALU

Each subject has two records, one for each parameter:
- PARAMCD "MRCMAXGM", PARAM "Geometric Mean Metabolite Ratio Cmax", AVALU
  "RATIO".
- PARAMCD "C2HREPSD", PARAM "Replicate SD of 2 H Concentration", AVALU
  "ng/mL".

For MRCMAXGM, AVAL is the geometric mean, across the periods, of the
subject's MRCMAX metabolite ratios (PPSTRESN) above zero: the exponential of
the mean of their natural logarithms. A ratio of zero, from a metabolite
concentration below the limit of quantification, has no logarithm and is left
out, as is a ratio that was not calculated.

For C2HREPSD, AVAL is the sample standard deviation, with divisor one less
than the number of assays, of the subject's replicate concentrations
(PCSTRESN) at planned time point "2H". AVAL has no value when the subject's
2-hour sample was assayed once; that subject still has its C2HREPSD record.
AVAL is not rounded for either parameter.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
