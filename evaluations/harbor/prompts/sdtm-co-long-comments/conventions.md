Following CDISC SDTM standards, use the provided CO_RAW dataset to create a CO
dataset with one record per subject per collected comment number.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, COSEQ, COVAL, COVAL1, COVAL2

DOMAIN is "CO". The comment text is held in 200-character pieces cut at fixed
character positions, not between words.

Read the source datasets from /app/input and save the completed dataset as
/app/output/co.csv.
