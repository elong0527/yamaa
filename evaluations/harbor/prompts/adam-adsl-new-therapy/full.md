Following CDISC ADaM standards, use the provided ADSL, CM, and PR datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTSDT, NACTDT, NACTDY, NACTFL

A medication record qualifies when its CMCAT is "ON TREATMENT", and a
procedure record qualifies when its PRCAT is "CANCER RELATED" and its
PRSCAT is "ON TREATMENT". Therapy recorded as "PRIOR TREATMENT" does not
qualify, and neither does a procedure given for a reason other than the
cancer. A qualifying record counts only when its start date is on or
after TRTSDT. A qualifying record with no start date does not count.
Where a medication start is recorded as a year and month only, the start
date is the 15th of that month; a full recorded start date stands as
recorded. NACTDT is the earlier of the subject's earliest qualifying
medication start date and earliest qualifying procedure start date.
NACTDT has no value when no qualifying record counts for the subject.
NACTDY is the study day of NACTDT counted from TRTSDT, and has no value
when NACTDT has no value. NACTFL is "Y" when NACTDT has a value and has
no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
