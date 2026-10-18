Following CDISC SDTM standards, use the provided DM_RAW dataset to create a
SUPPDM dataset with one record per subject per supplemental qualifier.

The output dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL

A subject has one record for each reported value of RACE1, RACE2 and
RACEOTH, and none for a value that was not reported. QNAM is the source
column's name, QLABEL is 'Race 1', 'Race 2' or 'Race, Other' respectively,
and QVAL is the value exactly as entered. RDOMAIN is 'DM' and QORIG is
'Collected'; IDVAR, IDVARVAL and QEVAL have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/suppdm.csv.
