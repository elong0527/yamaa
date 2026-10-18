Following CDISC SDTM standards, use the provided ODM dataset to create a DM
dataset with one record per subject and a SUPPDM dataset with one record
per reported race.

The DM dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, RACE, ETHNIC
The SUPPDM dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG, QEVAL

DOMAIN is "DM" and RDOMAIN is "DM". STUDYID is the ODM study OID and
USUBJID is the ODM subject key, each as given. SUBJID is the same value
as USUBJID. A race marked twice counts once. RACE is "MULTIPLE" when a
subject marked several distinct races. When a subject marked one race,
RACE is "WHITE" for "White", "ASIAN" for "Asian", "BLACK OR AFRICAN
AMERICAN" for "Black or African American", and "AMERICAN INDIAN OR
ALASKA NATIVE" for "American Indian or Alaska Native". RACE is "OTHER"
for "Other, specify: Fijian", "NOT REPORTED" for "Not reported", and
"UNKNOWN" for "Subject refused". RACE has no value when no race was
marked or the one marked answer is not one of the answers above.
ETHNIC is "HISPANIC OR LATINO" for "Hispanic or Latino", "NOT HISPANIC
OR LATINO" for "Not Hispanic or Latino", "NOT REPORTED" for "Not
reported", and "UNKNOWN" for "Unknown". ETHNIC has no value when no
ethnicity was collected or the collected answer is not one of the
answers above. SUPPDM holds one record for each distinct race a subject
marked, for exactly the subjects whose RACE is "MULTIPLE". IDVAR is
"USUBJID" and IDVARVAL is the subject's USUBJID. A subject's records
are numbered in alphabetical order of the collected answer: QNAM is
"RACE1", "RACE2", and so on, and QLABEL is "Race 1", "Race 2", and so
on. QVAL is that race in controlled terms, as in RACE. QORIG is "CRF"
and QEVAL has no value.

Read the source datasets from /app/input and save the completed DM dataset as
/app/output/dm.csv and the completed SUPPDM dataset as /app/output/suppdm.csv.
