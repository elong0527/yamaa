Following CDISC ADaM standards, use the provided ADSL, ADRS_RAW, and DS
datasets to create an ADTTE dataset for duration of response, with one
record per responder. A responder is a subject with a response date
(RESPDT) in ADSL.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ

PARAMCD is "DOR" and PARAM "Duration of Response". AVAL counts both STARTDT
and ADT. CNSR is 0 for an event and 1 for a censored record. EVNTDESC is
DISEASE PROGRESSION, DEATH, or CENSORED; CNSDTDSC is LAST TUMOUR ASSESSMENT,
START OF NEW ANTI-CANCER THERAPY, or has no value. SRCDOM, SRCVAR, and
SRCSEQ are ADRS, ADT, and the assessment's ASEQ; DS, DSSTDTC, and DSSEQ; or
ADSL, NACTDT, and no sequence.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
