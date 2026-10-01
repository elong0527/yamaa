Following CDISC ADaM standards, use the provided ADSL, RS, and DS
datasets to create an ADTTE dataset for progression-free survival,
with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
SRCDOM, SRCVAR, SRCSEQ

Use PARAMCD "PFS" and PARAM "Progression-Free Survival".

AVAL is the number of days from STARTDT to ADT, counting STARTDT as day
one. CNSR is 0 for an event and 1 for a censored record.

Describe each record with EVNTDESC DISEASE PROGRESSION, DEATH, or
CENSORED.

Trace ADT with SRCDOM, SRCVAR, and SRCSEQ, as one of: DS, DSDTC, and the
disposition sequence; or RS, RSDTC, and the response sequence.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
