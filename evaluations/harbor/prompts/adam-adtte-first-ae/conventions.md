Following CDISC ADaM standards, use the provided ADSL and ADAE datasets
to create an ADTTE dataset for time to first adverse event, with one
record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
SRCDOM, SRCVAR, SRCSEQ

Use PARAMCD "TTAE" and PARAM "Time to First Adverse Event".

AVAL is the number of days from STARTDT to ADT, counting both days.

CNSR is 0 for an event and 1 for a censored record. Describe each record
with EVNTDESC AE or END OF STUDY, from SRCDOM ADAE with SRCVAR ASTDT or
from SRCDOM ADSL with SRCVAR EOSDT. SRCSEQ is the chosen event's
sequence number, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
