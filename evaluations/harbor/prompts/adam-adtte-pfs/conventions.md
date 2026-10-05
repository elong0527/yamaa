Following CDISC ADaM standards, use the provided ADSL, RS, and DS
datasets to create an ADTTE dataset for progression-free survival,
with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
SRCDOM, SRCVAR, SRCSEQ

PARAMCD is "PFS" and PARAM "Progression-Free Survival". AVAL counts STARTDT
as day one. CNSR is 0 for an event and 1 for a censored record. EVNTDESC is
DISEASE PROGRESSION, DEATH, or CENSORED. A progression or death dated after
the ADSL new-therapy start date (NTXSTDT) is not an event; the subject is
then censored at the last adequate assessment dated on or before NTXSTDT,
or at STARTDT when no adequate assessment is usable. SRCDOM, SRCVAR, and
SRCSEQ are DS, DSDTC, and the disposition sequence; or RS, RSDTC, and the
response sequence; empty when censored at randomization.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
