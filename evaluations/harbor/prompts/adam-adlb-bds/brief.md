Following CDISC ADaM standards, use the provided LB and ADSL datasets
to create an ADLB dataset with one record per subject per parameter
per collection date.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRTSDT, TRT01A, AVAL, AVALU,
ABLFL, BASE, CHG, PCHG, ASEQ

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
