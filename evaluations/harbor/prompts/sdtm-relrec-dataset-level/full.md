Following CDISC SDTM standards, use the provided TU, TR, AE, and CM
datasets to create a RELREC dataset with one row per dataset-level
relationship and per related record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RDOMAIN, IDVAR, IDVARVAL, RELTYPE, RELID

RDOMAIN names the related domain. A dataset-level row relates whole
domains rather than subject records: USUBJID has no value, IDVAR names the
link variable (TULNKID or TRLNKID), IDVARVAL has no value, and RELTYPE
carries ONE on the TU row and MANY on the TR row, since one tumor
identification relates to many tumor results. The dataset-level rows share
RELID 1.

A record-level row names the subject in USUBJID, the sequence variable in
IDVAR (AESEQ or CMSEQ), and the sequence number of the related record as
text in IDVARVAL; RELTYPE has no value. RELID is the record's link
identifier, so records with different link identifiers land in distinct
relationships. An AE or CM record without a link identifier gets no row.

Rows sharing a RELID are related to one another, whether they point at
whole datasets or single records.

Read the source datasets from /app/input and save the completed dataset as
/app/output/relrec.csv.
