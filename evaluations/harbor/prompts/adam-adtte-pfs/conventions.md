Following CDISC ADaM standards, use the provided ADSL, DS, and RS datasets to
create an ADTTE dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, SRCDOM,
SRCVAR, SRCSEQ

PARAMCD is "PFS" and PARAM is "Progression-Free Survival" for every record.
AVAL is the number of days from STARTDT through ADT, counting the
randomization day as day one. CNSR is 0 or 1. EVNTDESC is "DISEASE
PROGRESSION", "DEATH", or "CENSORED". SRCDOM is "DS" or "RS", or has no
value. SRCVAR is "DSDTC" when SRCDOM is "DS" and "RSDTC" when SRCDOM is
"RS", or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
