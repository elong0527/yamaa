Following CDISC SDTM standards, use the provided MH and MH_SUPP_RAW datasets to
create a SUPPMH dataset with one record per collected qualifier.

The output dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL

RDOMAIN is "MH" and IDVAR is "MHSEQ" on every record. Each collected
qualifier on a condition gives one record, so a condition with both
qualifiers collected gives two records and a condition with only one
gives one. A qualifier whose answer is blank is not collected and gives
no record. IDVARVAL is the parent MH record's sequence number as text,
for the parent record with the same subject and the same condition term
as the qualifier. A parent MH condition with no collected qualifiers
gives no record. The family history qualifier has QNAM "MHFAMHX" and
QLABEL "Family History". The medical record confirmation qualifier has
QNAM "MHCONF" and QLABEL "Confirmed by Medical Records". QVAL is the
collected answer, "Y" or "N". QORIG is "Collected" on every record.
QEVAL has no value on every record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/suppmh.csv.
