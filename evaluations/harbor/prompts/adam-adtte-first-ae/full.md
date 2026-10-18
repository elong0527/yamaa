Following CDISC ADaM standards, use the provided ADAE and ADSL datasets to
create an ADTTE dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, SRCDOM,
SRCVAR, SRCSEQ

PARAMCD is "TTAE" and PARAM is "Time to First Adverse Event" on every
record. STARTDT is the subject's treatment start date and has no value
when the subject never started treatment. ADT is the earliest adverse
event onset date, or the end-of-study date when no event carries a
usable onset date. An event with no onset date cannot start the clock:
it is passed over when a dated event exists, and when it is the only
event the subject is censored at the end of study. ADT has no value when
neither an onset date nor an end-of-study date exists. A date before
the treatment start is moved up to the treatment start; the record keeps
its event or censoring status and source, and its day count is 1. AVAL
is the number of whole days from STARTDT through ADT, counting both
endpoints, and has no value when either date is missing. For an event,
CNSR is 0, EVNTDESC is "AE", SRCDOM is "ADAE", and SRCVAR is "ASTDT".
For a censored record, CNSR is 1, EVNTDESC is "END OF STUDY", SRCDOM is
"ADSL", and SRCVAR is "EOSDT". SRCSEQ is the chosen event's sequence
number, the lower number when several events share the earliest onset
date, and has no value for a censored record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
