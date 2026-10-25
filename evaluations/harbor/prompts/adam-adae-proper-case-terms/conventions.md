Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event record in AE.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AETERMPC, AETERMPB

AETERMPC and AETERMPB hold the reported term AETERM in proper case, or are
"Not Reported". AETERMPC's word separators are the blank, slash, hyphen, open
parenthesis, period and tab; AETERMPB's are the blank, slash, hyphen, open
parenthesis, period, comma and open bracket.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
