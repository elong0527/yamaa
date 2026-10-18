Following CDISC SDTM standards, use the provided DM_RAW dataset to create a
SUPPDM dataset with one record per subject per supplemental qualifier.

The output dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL

QNAM is RACE1, RACE2 or RACEOTH, and QLABEL is 'Race 1', 'Race 2' or 'Race,
Other' respectively. RDOMAIN is 'DM' and QORIG is 'Collected'; IDVAR,
IDVARVAL and QEVAL have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/suppdm.csv.
