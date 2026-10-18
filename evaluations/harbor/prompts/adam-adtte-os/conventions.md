Following CDISC ADaM standards, use the provided ADRS and ADSL datasets to
create an ADTTE dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC,
SRCDOM, SRCVAR, SRCSEQ

PARAMCD is "OS" and PARAM is "Overall Survival" on every record. AVAL is
the number of days from STARTDT through ADT, counting both endpoints. CNSR
is 0 for a qualifying death and 1 otherwise. EVNTDESC is "Death", "Alive",
or "Randomization". CNSDTDSC is "Alive During Study" or "Randomization",
or has no value. SRCDOM is "ADRS" or "ADSL". SRCVAR is "ADT", "LSTALVDT",
or "RANDDT".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
