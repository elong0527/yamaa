Following CDISC ADaM standards, use the provided ADSL and ADAE datasets
to create an ADTTE dataset for time to first adverse event, with one
record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
SRCDOM, SRCVAR, SRCSEQ

Use PARAMCD "TTAE" and PARAM "Time to First Adverse Event".

STARTDT is the treatment start date, and has no value when the subject
never started treatment.

ADT is the earliest adverse event onset date, or the end-of-study date
when no event carries a usable onset date. A date before treatment
start is moved up to it, keeping its event-or-censoring status and
source; with no treatment start the date is kept as it is. ADT has no
value when neither source date exists.

AVAL is the number of days from STARTDT to ADT, counting both days; it
has no value when either date has none.

CNSR is 0 for an event and 1 for a censored record. Describe each
record with EVNTDESC AE or END OF STUDY, from SRCDOM ADAE with SRCVAR
ASTDT or from SRCDOM ADSL with SRCVAR EOSDT. SRCSEQ is the chosen
event's sequence number, the lower number when several events share
the earliest onset date, and has no value for a censored record.

An event with no onset date is passed over when a dated event exists;
when it is the only event the subject is censored at the end of study.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
