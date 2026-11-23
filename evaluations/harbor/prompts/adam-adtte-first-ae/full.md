Following CDISC ADaM standards, use the provided ADAE and ADSL datasets to
create an ADTTE dataset with one record per treated subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, SRCDOM,
SRCVAR, SRCSEQ

PARAMCD is "TTAE" and PARAM is "Time to First Adverse Event" on every
record. A subject who never started treatment, with no TRTSDT, has no
record. STARTDT is the subject's treatment start date. ADT is the
earliest adverse event onset date. When no event carries a usable onset
date, the subject is censored: ADT is the end-of-study date (EOSDT), or
the last known alive date (LSTALVDT) when the subject has no end-of-study
date. An event with no onset date cannot start the clock: it is passed
over when a dated event exists, and when it is the only event the subject
is censored. A date before the treatment start is moved up to the
treatment start; the record keeps its event or censoring status and
source, and its day count is 1. AVAL is the number of whole days from
STARTDT through ADT, counting both endpoints. For an event, CNSR is 0,
EVNTDESC is "AE", SRCDOM is "ADAE", and SRCVAR is "ASTDT". For a record
censored at the end of study, CNSR is 1, EVNTDESC is "END OF STUDY",
SRCDOM is "ADSL", and SRCVAR is "EOSDT". For a record censored at the
last known alive date, CNSR is 1, EVNTDESC is "LAST KNOWN ALIVE", SRCDOM
is "ADSL", and SRCVAR is "LSTALVDT". SRCSEQ is the chosen event's sequence
number, the lower number when several events share the earliest onset
date, and has no value for a censored record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
