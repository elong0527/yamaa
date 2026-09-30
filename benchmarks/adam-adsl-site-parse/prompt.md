Following CDISC ADaM standards, use the provided DM dataset to create an
ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, SUBJID, SITEIDP, SITEID, SUBJREF

SITEIDP is the middle segment of the unique subject identifier when
the identifier holds two dashes with four digits after the last one;
it has no value when the identifier has any other shape.

SITEID is the site to use: the parsed site when present, otherwise the
collected site, otherwise UNKNOWN.

SUBJREF combines the site to use and the collected subject number with
a colon; it is UNKNOWN when the subject number is missing.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
