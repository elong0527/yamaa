Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AETERMPC, AETERMPB

AETERMPC is AETERM in proper case, as the SAS PROPCASE function writes it with
its default delimiters: every letter in lower case, then the first character
and each character that directly follows a blank, slash, hyphen, open
parenthesis, period or tab in upper case. A letter after any other character
stays in lower case, so "INVESTIGATOR'S DECISION" gives "Investigator's
Decision", "2ND DEGREE BURN" gives "2nd Degree Burn" and "PAIN,BACK" gives
"Pain,back". AETERMPB is the same with the delimiters blank, slash, hyphen,
open parenthesis, period, comma and open bracket, which replace the default
ones, so "PAIN,BACK" gives "Pain,Back" and "RASH [LEFT ARM]" gives
"Rash [Left Arm]". When AETERM has no value, AETERMPC and AETERMPB are both
"Not Reported".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
