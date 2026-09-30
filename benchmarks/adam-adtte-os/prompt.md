Following CDISC ADaM standards, use the provided ADSL and ADRS datasets
to create an ADTTE dataset for overall survival, with one record per
subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC,
CNSDTDSC, SRCDOM, SRCVAR, SRCSEQ

Use PARAMCD "OS" and PARAM "Overall Survival". STARTDT is the
randomization date, the origin from which survival time is counted.

A qualifying death is a dated response record with code DEATH, result
Y, and analysis flag Y; the earliest such date counts, with the lower
sequence number breaking a tie on the same date.

ADT is the death date when it falls on or after STARTDT, and STARTDT
itself when the death falls before it. With no death, ADT is the
last-alive date when that date falls after randomization, else the
randomization date. A death dated before randomization still counts as
an event.

AVAL is the number of days from STARTDT to ADT, counting both days.
CNSR is 0 for a death and 1 otherwise. EVNTDESC is Death for a death,
Alive when censoring at the last-alive date, and Randomization when
censoring at randomization. CNSDTDSC has no value for a death, and
holds Alive During Study or Randomization for a censored record.

Trace ADT with SRCDOM ADRS, SRCVAR ADT, and the death record sequence
for a death; with SRCDOM ADSL, SRCVAR LSTALVDT, and no sequence when
censoring at the last-alive date; and with SRCDOM ADSL and SRCVAR
RANDDT when censoring at randomization.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
