Following CDISC ADaM standards, use the provided DM dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMNT, CMNTFL

CMNT is the comment exactly as the investigator recorded it: a comma
inside it, quotation marks around a subject's own words, and a line
break in the middle of it all reach the result unchanged, and CMNT has
no value when no comment was collected. CMNTFL is "Y" when a comment
was collected and "N" when none was. A comment field that is empty,
quoted or not, or holds only spaces counts as no comment collected:
CMNTFL is "N" and CMNT has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
