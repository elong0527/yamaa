Following CDISC SDTM standards, use the provided MH_RAW dataset to create
a SUPPMH dataset with one supplemental record per collected qualifier.

The output dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG,
QEVAL

RDOMAIN is MH and IDVAR is MHSEQ. IDVARVAL is the parent record sequence,
written as text. QNAM is the qualifier name, MHFAMHX or MHCONF. QLABEL is
Family History for MHFAMHX and Confirmed by Medical Records for MHCONF.
QVAL is the collected qualifier value, Y or N. QORIG records the collected
origin as Collected. QEVAL has no value, since a collected value is not an
assessment.

Each parent record contributes one supplemental record per qualifier
actually collected, so a record with only one of the two collected
contributes only one.

Read the source datasets from /app/input and save the completed dataset as
/app/output/suppmh.csv.
