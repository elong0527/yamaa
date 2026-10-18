Following CDISC ADaM standards, use the provided DM dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, SUBJID, SITEIDP, SITEID, SUBJREF

SITEIDP is the site code in the middle of the unique subject identifier
when the identifier has three hyphen-separated parts, the last of exactly
four digits, and has no value when the identifier has any other shape.
SITEID is the site to use: the parsed site when present, otherwise the
collected site, otherwise "UNKNOWN". SUBJREF combines the site to use and
the collected subject number with a colon, and is "UNKNOWN" when the
subject number is missing.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
