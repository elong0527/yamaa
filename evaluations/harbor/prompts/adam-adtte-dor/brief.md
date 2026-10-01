Following CDISC ADaM standards, use the provided ADSL, ADRS_RAW, and DS
datasets to create an ADTTE dataset for duration of response, with one
record per responder. A responder is a subject with a response date
(RESPDT) in ADSL.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
