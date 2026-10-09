Following CDISC standards, use the provided SUBJ dataset to create a
CLINSITE dataset for a Bioresearch Monitoring (BIMO) submission package,
with one record per clinical site.

The output dataset should contain the following columns in this order:
STUDYID, SITEID, ARM, SAFPOP, ENRLPOP

SITEID is the site key (SITENUM) exactly as collected, with leading zeros
kept: '007' and '7' are different sites.

ARM is the alphabetically first arm among the site's treated subjects; it
has no value when the site has no treated subjects.

SAFPOP is the number of treated subjects at the site; ENRLPOP is the number
of enrolled subjects at the site, treated or not. A treated subject is one
whose treated flag (SAFFL) is 'Y'.

A site appears in the dataset exactly when at least one subject row names
it. A site with only untreated subjects still gets a record, with no ARM
value and SAFPOP of zero; a site named by no subject row gets no record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/clinsite.csv.
