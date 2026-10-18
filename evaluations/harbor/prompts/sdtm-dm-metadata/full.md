Following CDISC SDTM standards, use the provided DM_RAW dataset to create a DM
dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, SITEID, AGE, AGEU, SEX, COUNTRY

DOMAIN is "DM". USUBJID joins the study identifier, the site
identifier, and the subject identifier with hyphens. SITEID is the
collected site identifier. AGE is the collected age in whole years,
between 0 and 120, and has no value when no age was collected. AGEU
is "YEARS", since age is collected in whole years. SEX is the
collected sex, one of "F", "M", or "U". COUNTRY is the collected
three-letter country code.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
