Following CDISC SDTM standards, use the provided MH_SUPP_RAW and MH
datasets to create a SUPPMH dataset with one supplemental record per
collected qualifier.

The output dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG,
QEVAL

RDOMAIN is MH and IDVAR is MHSEQ. IDVARVAL is the parent sequence number as
text. QNAM is the qualifier name, MHFAMHX or MHCONF. QLABEL is Family History
for MHFAMHX and Confirmed by Medical Records for MHCONF. QVAL is Y or N. QORIG
is Collected for collected values. QEVAL has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/suppmh.csv.
