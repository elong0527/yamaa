Following CDISC ADaM standards, use the provided CM and DM datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RESCTRT

RESCTRT is the treatment name (CMTRT) of the subject's first rescue
medication, or has no value, and it is never filled with placeholder
text.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
