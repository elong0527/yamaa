Following CDISC ADaM standards, use the provided ADRS_RAW, ADSL, and DS
datasets to create an ADTTE dataset with one record per responder.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC,
SRCDOM, SRCVAR, SRCSEQ

PARAMCD is "DOR" and PARAM is "Duration of Response" on every record.
AVAL counts both endpoints. EVNTDESC is "CENSORED", "DISEASE PROGRESSION"
or "DEATH". An event after the start of new anti-cancer therapy (NACTDT)
does not count. A censored record is censored at the last assessment
with a response other than "NE", dated on or before NACTDT when present,
or at the response date when there is none. CNSDTDSC is "LAST TUMOUR
ASSESSMENT", "RESPONSE DATE", or has no value. SRCDOM is "ADRS", "DS",
or "ADSL", and SRCVAR is "DSSTDTC" for "DS", "ADT" for "ADRS", and
"RESPDT" otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
