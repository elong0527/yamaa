Following CDISC ADaM standards, use the provided TRT and EX datasets to
create an ADEX dataset with one record per subject per treatment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXTRT, DOSECUM, NDOSREC, NDOSVAL

DOSECUM is the total administered dose across the treatment's exposure
records; empty when no dose was ever recorded, while a recorded zero
total is kept so a measured zero never reads as missing. NDOSREC is
the number of exposure records for the treatment; empty when the
treatment has no exposure record at all. NDOSVAL is the number of
exposure records carrying a recorded dose; an explicitly recorded zero
counts as a dose, and the count is zero (not empty) when records exist
but every dose was left blank.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
