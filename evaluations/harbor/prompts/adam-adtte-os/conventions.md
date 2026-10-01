Following CDISC ADaM standards, use the provided ADSL and ADRS datasets
to create an ADTTE dataset for overall survival, with one record per
subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ

Use PARAMCD "OS" and PARAM "Overall Survival".

AVAL is the number of days from STARTDT to ADT, counting both days. CNSR
is 0 for a death and 1 otherwise. EVNTDESC is Death, Alive, or
Randomization. CNSDTDSC holds Alive During Study or Randomization, or
has no value.

Trace ADT with SRCDOM, SRCVAR, and SRCSEQ, as one of: ADRS, ADT, and the
death record sequence; ADSL, LSTALVDT, and no sequence; or ADSL and
RANDDT.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
