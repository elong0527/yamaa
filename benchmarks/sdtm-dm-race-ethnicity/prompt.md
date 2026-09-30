Following CDISC SDTM standards, use the provided ODM dataset to create
a DM dataset with one record per subject and a SUPPDM dataset with one
record per reported race for each subject who marked several.

The DM dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, RACE, ETHNIC
The SUPPDM dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG,
QEVAL

RACE maps the collected answer to controlled terms: White gives WHITE,
Asian gives ASIAN, Black or African American gives BLACK OR AFRICAN
AMERICAN, American Indian or Alaska Native gives AMERICAN INDIAN OR
ALASKA NATIVE, Native Hawaiian or Other Pacific Islander gives NATIVE
HAWAIIAN OR OTHER PACIFIC ISLANDER, a listed free-text answer outside
the named races gives OTHER, a refused answer gives UNKNOWN, and a
recorded non-answer gives NOT REPORTED. Several distinct races give
MULTIPLE. RACE has no value when no race was marked or the single
answer is not in the list. A race marked twice counts once. ETHNIC maps
the collected answer: Hispanic or Latino gives HISPANIC OR LATINO, Not
Hispanic or Latino gives NOT HISPANIC OR LATINO, Not reported gives NOT
REPORTED, and Unknown gives UNKNOWN. ETHNIC has no value when no
ethnicity was collected or its answer is not in the list. SUBJID
repeats USUBJID. SUPPDM carries records exactly for the subjects whose
DM record says MULTIPLE. IDVAR is USUBJID and IDVARVAL is the subject
identifier. QNAM is RACE1, RACE2, and so on, with QLABEL Race 1,
Race 2, and so on, numbered in alphabetical order of the collected
answer. QVAL maps the race as RACE does. QORIG is always CRF, and QEVAL
has no value. RDOMAIN is DM.

Read the source datasets from /app/input and save the completed DM
dataset as /app/output/dm.csv and the completed SUPPDM dataset as
/app/output/suppdm.csv.
