Following CDISC SDTM standards, use the provided ODM dataset to create
a DM dataset with one record per subject and a SUPPDM dataset with one
record per reported race for each subject who marked several.

The DM dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, RACE, ETHNIC
The SUPPDM dataset should contain the following columns in this order:
STUDYID, RDOMAIN, USUBJID, IDVAR, IDVARVAL, QNAM, QLABEL, QVAL, QORIG,
QEVAL

Each collected race answer maps to a controlled term:
- "White", "Asian", "Black or African American", "American Indian or
  Alaska Native", and "Native Hawaiian or Other Pacific Islander" give
  the same name in upper case;
- "Other, specify: Fijian" gives OTHER;
- "Subject refused" gives UNKNOWN;
- "Not reported" gives NOT REPORTED;
- any other answer is not in the list and gives no value.

Every distinct answer a subject gave counts as one race, a non-answer
such as "Not reported" included, and a race marked twice counts once.
RACE is MULTIPLE when a subject has two or more races, and otherwise the
mapped term of the single answer; it has no value when no race was
marked or the single answer is not in the list. ETHNIC maps the
collected answer: "Hispanic or Latino" gives HISPANIC OR LATINO, "Not
Hispanic or Latino" gives NOT HISPANIC OR LATINO, "Not reported" gives
NOT REPORTED, and "Unknown" gives UNKNOWN. ETHNIC has no value when no
ethnicity was collected or its answer is not in the list. SUBJID
repeats USUBJID.

SUPPDM carries records exactly for the subjects whose DM record says
MULTIPLE, one per race. RDOMAIN is DM, IDVAR is USUBJID, and IDVARVAL is
the subject identifier. QNAM is RACE1, RACE2, and so on, with QLABEL
Race 1, Race 2, and so on, numbered in alphabetical order of the
collected answer. QVAL maps the race as RACE does. QORIG is always CRF,
and QEVAL has no value.

Read the source datasets from /app/input and save the completed DM
dataset as /app/output/dm.csv and the completed SUPPDM dataset as
/app/output/suppdm.csv.
