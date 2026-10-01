Following CDISC SDTM standards, use the provided ODM dataset to create
a DM dataset with one record per subject and a SUPPDM dataset with one
record per reported race for each subject who marked several.

The DM dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, RACE, ETHNIC
The SUPPDM dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG,
QEVAL

Read the source datasets from /app/input and save the completed DM
dataset as /app/output/dm.csv and the completed SUPPDM dataset as
/app/output/suppdm.csv.
