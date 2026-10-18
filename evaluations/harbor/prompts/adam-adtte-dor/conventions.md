Following CDISC ADaM standards, use the provided ADRS_RAW, ADSL, and DS
datasets to create an ADTTE dataset with one record per responder.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC,
SRCDOM, SRCVAR, SRCSEQ

PARAMCD is "DOR" and PARAM is "Duration of Response" on every record.
AVAL is the number of days from STARTDT through ADT, counting both
endpoints. CNSR is 0 or 1. EVNTDESC is "CENSORED", "DISEASE
PROGRESSION", or "DEATH". CNSDTDSC is "LAST TUMOUR ASSESSMENT", "START
OF NEW ANTI-CANCER THERAPY", or has no value. SRCDOM is "ADRS", "DS",
or "ADSL". SRCVAR is "DSSTDTC" when SRCDOM is "DS", "ADT" when SRCDOM
is "ADRS", and "NACTDT" otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
