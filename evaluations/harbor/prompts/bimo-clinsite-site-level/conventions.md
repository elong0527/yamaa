Following CDISC standards, use the provided SUBJ dataset to create a
CLINSITE dataset for a Bioresearch Monitoring (BIMO) submission package,
with one record per clinical site.

The output dataset should contain the following columns in this order:
STUDYID, SITEID, ARM, SAFPOP, ENRLPOP

SITEID keeps leading zeros: '007' and '7' are different sites.

ARM is the alphabetically first arm among the site's treated subjects, or
has no value.

SAFPOP counts treated subjects ('Y'); ENRLPOP counts all enrolled subjects.

Read the source datasets from /app/input and save the completed dataset as
/app/output/clinsite.csv.
