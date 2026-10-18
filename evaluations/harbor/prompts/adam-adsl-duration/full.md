Following CDISC ADaM standards, use the provided DM dataset to create an ADSL
dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, STDT, ENDT, DURW, DURM

DURW is the count of whole seven-day blocks from STDT to ENDT; a leftover
partial week adds nothing. DURM is the count of monthly anniversaries of
STDT falling on or before ENDT, where an anniversary falls on the same day
of the month as STDT, or on the last day of the month when that month has
no such day. DURW and DURM have no value when STDT or ENDT has no value.
When ENDT falls before STDT, each duration is the negative of the count
with the two dates exchanged.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
