Following CDISC SDTM standards, use the provided ODM dataset to create a DM
dataset with one record per subject and a SUPPDM dataset with one record
per reported race.

The DM dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, RACE, ETHNIC
The SUPPDM dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL

DOMAIN is "DM" and RDOMAIN is "DM". Each collected race answer gives the
same name in upper case, except that "Other, specify: Fijian" gives
"OTHER" and "Subject refused" gives "UNKNOWN"; RACE also has no value.
Each collected ethnicity answer gives the same name in upper case; ETHNIC
also has no value. IDVAR is "USUBJID". QNAM is "RACE1", "RACE2", and so
on, and QLABEL is "Race 1", "Race 2", and so on. QORIG is "CRF" and QEVAL
has no value.

Read the source datasets from /app/input and save the completed DM dataset as
/app/output/dm.csv and the completed SUPPDM dataset as /app/output/suppdm.csv.
