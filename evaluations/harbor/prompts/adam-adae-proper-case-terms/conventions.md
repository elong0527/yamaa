Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AETERMPC, AETERMPB

AETERMPC and AETERMPB are AETERM in proper case, as the SAS PROPCASE function
writes it, or "Not Reported": AETERMPC with the default delimiters, and
AETERMPB with the delimiters blank, slash, hyphen, open parenthesis, period,
comma and open bracket.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
