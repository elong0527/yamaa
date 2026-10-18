Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AESPID, AEREFNUM, AETERM, AETERMLO, AERELLC, AREL

AEREFNUM is the number taken from AESPID when AESPID consists of "AE-" followed
by exactly three digits, so "AE-001" gives 1. A missing AESPID gives 0, and an
AESPID of any other shape gives -1. The pattern accepts exactly three digits,
so "AE-000" gives 0, while "AE-1000" and a lower-case "ae-007" each give -1.
AETERMLO is AETERM in lower case. AERELLC is AEREL in lower case, and a missing
AEREL gives "not reported". AREL is AERELLC in upper case: "RELATED", "POSSIBLY
RELATED", "NOT RELATED", or "NOT REPORTED".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
