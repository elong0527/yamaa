Following CDISC SDTM standards, use the provided CO_RAW dataset to create a CO
dataset with one record per collected comment.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, COSEQ, COVAL, COVAL1, COVAL2

The comment is cut into pieces of 200 characters at exact character
positions, not between words.

Read the source datasets from /app/input and save the completed dataset as
/app/output/co.csv.
