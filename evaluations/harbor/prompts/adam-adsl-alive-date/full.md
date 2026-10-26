Following CDISC ADaM standards, use the provided AE, DM, DS, EX, SS, SV, and VS
datasets to create an ADSL dataset with one record per subject in DM.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, LSTALVDT, DTHFL, DTHDT, DTHDTF

A date shows the subject alive when it is an exposure start or end (EXSTDTC,
EXENDTC), an adverse event start or end (AESTDTC, AEENDTC), a disposition
date (DSSTDTC) except on a record whose DSDECOD is "DEATH" or
"LOST TO FOLLOW-UP", a visit start (SVSTDTC), a vital signs date (VSDTC), or
a survival status date (SSDTC) whose SSSTRESC is "ALIVE". Only these dates
count, and the death date is not one of them. Each date counts on the day it
names: a date with a time counts on its date, a year and month on the first
of that month, and a year alone on 1 January.

- LSTALVDT is the latest date showing the subject alive, but never later
  than DTHDT: when that latest date falls after DTHDT, LSTALVDT is DTHDT.
- DTHFL is "Y" for a subject DM records as dead, including one without a
  collected death date, and has no value otherwise.
- DTHDT is the death date collected in DM (DTHDTC), completed to a day; no
  other domain supplies it, and it has no value when DM collected none. A
  complete date stands as collected. A year and month take the first day of
  that month and a year alone 1 January, unless the latest date showing the
  subject alive falls on a later day inside the collected month or year:
  then DTHDT is that day.
- DTHDTF is "D" when the day of DTHDT was supplied (a year and month were
  collected) and "M" when its month and day were (a year alone was
  collected). The flag is the same when DTHDT takes the day of the latest
  date showing the subject alive. It has no value for a complete or missing
  death date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
