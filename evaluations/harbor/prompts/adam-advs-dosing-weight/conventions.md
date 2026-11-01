Following CDISC ADaM standards, use the provided ADSL and VS datasets to create
an ADVS dataset with one record per subject per treatment cycle.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, AVISITN, ADT, AVAL, ANCHORWT, BSA

BSA is rounded to two decimal places.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
