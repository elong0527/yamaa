Following CDISC SDTM standards, use the provided MH and MH_SUPP_RAW datasets to
create a SUPPMH dataset with one record per collected qualifier.

The output dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL

RDOMAIN is "MH" and IDVAR is "MHSEQ" on every record. The family
history qualifier has QNAM "MHFAMHX" and QLABEL "Family History". The
medical record confirmation qualifier has QNAM "MHCONF" and QLABEL
"Confirmed by Medical Records". QVAL is "Y" or "N". QORIG is
"Collected" on every record. QEVAL has no value on every record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/suppmh.csv.
