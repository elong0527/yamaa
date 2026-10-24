Following CDISC ADaM standards, use the provided PC and PP datasets to create
an ADPP dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, AVALU

PARAMCD is "MRCMAXGM" with PARAM "Geometric Mean Metabolite Ratio Cmax" and
AVALU "RATIO", or "C2HREPSD" with PARAM "Replicate SD of 2 H Concentration"
and AVALU "ng/mL". AVAL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
