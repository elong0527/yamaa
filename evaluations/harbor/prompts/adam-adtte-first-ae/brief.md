Following CDISC ADaM standards, use the provided ADSL and ADAE datasets
to create an ADTTE dataset for time to first adverse event, with one
record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
SRCDOM, SRCVAR, SRCSEQ

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
