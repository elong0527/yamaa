Following CDISC ADaM standards, use the provided ADSL, QS, and DS datasets
to create an ADTTE dataset for time to PRO deterioration, one record per
subject.

The ADTTE dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
