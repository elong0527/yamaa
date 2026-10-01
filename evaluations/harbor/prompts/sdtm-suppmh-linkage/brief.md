Following CDISC SDTM standards, use the provided MH_SUPP_RAW and MH
datasets to create a SUPPMH dataset with one supplemental record per
collected qualifier.

The output dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG,
QEVAL

Read the source datasets from /app/input and save the completed dataset as
/app/output/suppmh.csv.
