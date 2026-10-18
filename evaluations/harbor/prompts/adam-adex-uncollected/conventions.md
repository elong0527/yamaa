Following CDISC ADaM standards, use the provided EX and SUBJECT_TREATMENT
datasets to create an ADEX dataset with one record per subject and treatment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXTRT, DOSECUM, NDOSREC, NDOSVAL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
