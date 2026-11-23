Following CDISC ADaM standards, use the provided ADAE and ADSL datasets to
create an ADTTE dataset with one record per treated subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, SRCDOM,
SRCVAR, SRCSEQ

PARAMCD is "TTAE" and PARAM is "Time to First Adverse Event" on every
record. AVAL is the number of whole days from STARTDT through ADT,
counting both endpoints. CNSR is 0 or 1. EVNTDESC is "AE", "END OF
STUDY" or "LAST KNOWN ALIVE". SRCDOM is "ADAE" or "ADSL". SRCVAR is
"ASTDT", "EOSDT" or "LSTALVDT".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
