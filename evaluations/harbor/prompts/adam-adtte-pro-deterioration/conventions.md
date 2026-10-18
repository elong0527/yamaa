Following CDISC ADaM standards, use the provided ADSL, DS, and QS datasets to
create an ADTTE dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC,
SRCDOM, SRCVAR, SRCSEQ

PARAMCD is "TTDGHS" and PARAM is "Time to Deterioration in Global Health
Status". AVAL is the number of whole calendar months from STARTDT to ADT.
CNSR is 0 or 1. EVNTDESC is "PRO DETERIORATION", "DEATH", or "CENSORED".
CNSDTDSC holds the censoring reason, or is "STUDY COMPLETION", or has no
value. SRCDOM is "QS" or "ADSL".
SRCVAR is "ADT", "DTHDT", or "RANDDT".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
