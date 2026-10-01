Following CDISC SDTM standards, use the provided ODM dataset to create
a DM dataset with one record per subject and a SUPPDM dataset with one
record per reported race for each subject who marked several.

The DM dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, RACE, ETHNIC
The SUPPDM dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG,
QEVAL

Each collected race answer gives the same name in upper case, except that
"Other, specify: Fijian" gives OTHER and "Subject refused" gives UNKNOWN.
ETHNIC maps the collected answer to the same name in upper case.

RDOMAIN is DM and IDVAR is USUBJID. QNAM is RACE1, RACE2, and so on, with
QLABEL Race 1, Race 2, and so on, numbered in alphabetical order of the
collected answer. QVAL maps the race as RACE does. QORIG is always CRF, and
QEVAL has no value.

Read the source datasets from /app/input and save the completed DM
dataset as /app/output/dm.csv and the completed SUPPDM dataset as
/app/output/suppdm.csv.
