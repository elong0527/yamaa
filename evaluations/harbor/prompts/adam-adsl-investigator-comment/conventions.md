Following CDISC ADaM standards, use the provided CO and DM datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMNT, CMNTFL

A comment split across COVAL and COVAL1 joins back with one space, and a
subject's comments join in COSEQ order, separated by a semicolon and a
space ("; "). CMNT keeps commas, quotation marks and line breaks
unchanged, or has no value. CMNTFL is "Y" or "N".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
