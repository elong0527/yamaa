Following CDISC ADaM standards, use the provided DM dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMNT, CMNTFL

CMNT is the comment exactly as the investigator recorded it: a comma
inside it, quotation marks around a subject's own words, and a line
break in the middle of it all reach the result unchanged, and CMNT has
no value. CMNTFL is "Y" or "N".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
