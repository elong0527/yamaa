Following CDISC ADaM standards, use the provided TRT and EX datasets to
create an ADEX dataset with one record per subject per treatment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXTRT, EXDOSU, DOSECUM, NCYCLES, RDI

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
