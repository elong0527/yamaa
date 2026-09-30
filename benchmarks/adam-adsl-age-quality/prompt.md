Following CDISC ADaM standards, use the provided DM dataset to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AGE

AGE is the subject's age in years as collected. A value below 18 or
above 100 stays in the dataset as collected, and a missing age stays
missing; neither case stops the run.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
