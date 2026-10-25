Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event record in AE.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AETERMPC, AETERMPB

AETERMPC and AETERMPB hold the reported term AETERM in proper case: every
letter is in lower case except the first character and each character
directly after one of the variable's word separators, which are in upper
case. A letter after any other character stays in lower case.
- AETERMPC's separators are the blank, slash, hyphen, open parenthesis,
  period and tab, so a letter after an apostrophe, a digit, a comma or an
  open bracket stays in lower case.
- AETERMPB's own list replaces that one: the blank, slash, hyphen, open
  parenthesis, period, comma and open bracket. A letter after a comma or an
  open bracket is therefore in upper case there, and the tab is not one of
  its separators.

Only the plain English letters A to Z change case; every other character
is unchanged. AETERMPC and AETERMPB are "Not Reported" when AETERM has no
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
