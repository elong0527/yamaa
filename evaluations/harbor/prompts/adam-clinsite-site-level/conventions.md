Following CDISC ADaM standards, use the provided ADSL dataset to create a
CLINSITE dataset with one record per clinical site.

The output dataset should contain the following columns in this order:
STUDYID, SITEID, ARM, SAFPOP, ENRLPOP

SAFFL is "Y". SITEID is the site key exactly as collected, leading zeros
kept, so a site key of "007" and a site key of "7" are different sites.

Read the source datasets from /app/input and save the completed dataset as
/app/output/clinsite.csv.
