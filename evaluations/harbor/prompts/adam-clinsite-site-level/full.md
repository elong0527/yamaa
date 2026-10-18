Following CDISC ADaM standards, use the provided ADSL dataset to create a
CLINSITE dataset with one record per clinical site.

The output dataset should contain the following columns in this order:
STUDYID, SITEID, ARM, SAFPOP, ENRLPOP

A subject counts as treated when its treated flag, SAFFL, is "Y".
A site appears in the dataset exactly when at least one subject row
names it, and a site named by no subject row does not appear.
SITEID is the site key exactly as collected, leading zeros kept, so a
site key of "007" and a site key of "7" are different sites.
ARM is the alphabetically first arm among the site's treated subjects,
and it has no value when the site has no treated subjects.
SAFPOP is the number of treated subjects at the site, and it is zero
for a site whose subjects were all untreated.
ENRLPOP is the number of enrolled subjects at the site, treated or not.

Read the source datasets from /app/input and save the completed dataset as
/app/output/clinsite.csv.
