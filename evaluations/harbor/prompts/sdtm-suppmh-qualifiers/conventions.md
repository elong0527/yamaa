Following CDISC SDTM standards, use the provided MH_RAW dataset to create a
SUPPMH dataset with one record per collected qualifier.

The output dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL

RDOMAIN is "MH" on every record. IDVAR is "MHSEQ" on every record.
IDVARVAL is the parent record's sequence number, written as text. On a
record for the family history qualifier, QNAM is "MHFAMHX" and QLABEL is
"Family History". On a record for the medical record confirmation
qualifier, QNAM is "MHCONF" and QLABEL is "Confirmed by Medical Records".
QVAL is "Y" or "N". QORIG is "Collected" on every record. QEVAL has no
value on any record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/suppmh.csv.
