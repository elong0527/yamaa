Following CDISC ADaM standards, use the provided IS dataset to create an ADIS
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISIT, AVAL

PARAMCD is "NABLOG10" and AVAL is not rounded. A titer below the lower
limit of quantitation gives no record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adis.csv.
