Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AESPID, AEREFNUM, AETERM, AETERMLO, AERELLC,
AREL

AEREFNUM is the number taken from an AESPID made of the upper-case
letters AE, a hyphen, and exactly three digits, so "AE-042" gives 42
and three zeroes give 0. A missing identifier gives 0, and any other
shape, including lower-case letters or more or fewer digits, gives -1.
AETERMLO is the reported term in lower case. AERELLC is the causality
in lower case; a missing causality gives "not reported". AREL is the
analysis causality in upper case: RELATED, POSSIBLY RELATED, NOT
RELATED, or NOT REPORTED.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
