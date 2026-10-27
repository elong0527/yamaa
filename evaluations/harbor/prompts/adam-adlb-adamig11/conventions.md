Following CDISC ADaM standards, use the provided ADSL and LB datasets to create
an ADLB dataset with one record per subject per parameter per collection date.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRT01A, ADY, AVAL, AVALU, ABLFL, BASE,
CHG

Use ADaMIG 1.1. PARAM is "Alanine Aminotransferase". ABLFL is "Y".
BASE and CHG are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
