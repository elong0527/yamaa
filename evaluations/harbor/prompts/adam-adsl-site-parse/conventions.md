Following CDISC ADaM standards, use the provided DM dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, SUBJID, SITEID, SITEIDP, SUBJREF

SITEID is the collected site in DM. SITEIDP is the site code in the middle
of the unique subject identifier, or has no value, and never replaces
SITEID. SUBJREF combines SITEID and the collected subject number with a
colon.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
