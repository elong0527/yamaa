Following CDISC ADaM standards, use the provided CO and DM datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMNT, CMNTFL

Each CO record is one investigator comment. COVAL holds the comment, and
a comment longer than 200 characters continues in COVAL1, split between
words, so the comment is COVAL followed by one space and COVAL1. A
COVAL1 that is empty, quoted or not, adds nothing. CMNT is the subject's
comments exactly as the investigator recorded them: a comma inside a
comment, quotation marks around a subject's own words, and a line break
in the middle of a comment all reach the result unchanged. When a
subject has several comments, CMNT joins them in COSEQ order, separated
by a semicolon and a space ("; "). CMNT has no value when the subject
has no CO record. CMNTFL is "Y" when the subject has a comment and "N"
when none was collected.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
