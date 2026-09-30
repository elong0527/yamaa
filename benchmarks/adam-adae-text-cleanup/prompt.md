Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AESPID, AEREFNUM, AETERM, AETERMLO, AERELLC,
AREL

AEREFNUM is the number taken from AESPID values shaped like "AE-001":
the letters AE, a hyphen, and exactly three digits. A missing
identifier gives 0 and any other shape gives -1, so "AE-000" gives 0
while "AE-1000" and a lowercase "ae-007" each give -1. AETERMLO is the
reported term in lower case. AERELLC is the causality in lower case; a
missing causality gives not reported. AREL is the analysis causality
in upper case: RELATED, POSSIBLY RELATED, NOT RELATED, or
NOT REPORTED.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
