Following CDISC ADaM standards, use the provided CM and DM datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RESCFL, RESCDT, RESCDTF, RESCTRT

A rescue medication is a CM record whose category (CMCAT) is "RESCUE
MEDICATION". RESCFL is "Y" when the subject has a rescue medication
record and "N" otherwise, including a subject with no medication
records at all. The first rescue medication is the subject's rescue
record with the earliest start date (CMSTDTC), with the smaller CMSEQ
breaking a tie between records with the same start date. A start date
recorded with only a year and month completes to the first day of that
month, and one recorded with only a year completes to January 1 of that
year, before start dates are compared. A rescue record with no start
date is the first rescue medication only when the subject has no rescue
record with a start date. A duplicated rescue record does not change
the answer.

RESCDT is the completed start date of the first rescue medication; it
has no value when that record has no start date or the subject took no
rescue medication. RESCDTF is "D" when only the day of RESCDT was
completed and "M" when the month and day were; it has no value when the
start date was recorded in full or RESCDT has no value. RESCTRT is the
standardized medication name (CMDECOD) of the first rescue medication;
it has no value when the subject took no rescue medication, and it is
never filled with placeholder text.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
