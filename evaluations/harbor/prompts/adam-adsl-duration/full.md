Following CDISC ADaM standards, use the provided DM dataset to create an
ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, STDT, ENDT, DURW, DURM

DURW is the count of whole seven-day blocks from STDT to ENDT; a
leftover partial week adds nothing. DURM is the count of monthly
anniversaries of STDT falling on or before ENDT; an anniversary keeps
the start day, or the last day of the month when the month is too
short.

Both durations have no value when either date is missing. When the end
date falls before the start date, each duration is the negated count
computed with the dates exchanged.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
