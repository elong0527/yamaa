Following CDISC ADaM standards, use the provided ADRS and ADSL datasets to
create an ADTTE dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, STARTDT, ADT, AVAL, CNSR, EVNTDESC, CNSDTDSC,
SRCDOM, SRCVAR, SRCSEQ

- PARAMCD is "OS" and PARAM is "Overall Survival" on every record.
- STARTDT is the randomization date (RANDDT), the origin from which
  survival time is counted.
- A qualifying death record is an ADRS record with PARAMCD "DEATH",
  AVALC "Y", ANL01FL "Y", and a recorded ADT. The subject's death is
  the qualifying record with the earliest ADT, and on a tied date the
  one with the lower ASEQ. A subject with no qualifying record has no
  death.
- ADT is the death date when a qualifying death falls on or after
  STARTDT, and STARTDT itself when the death falls before it; a death
  dated before randomization still counts as the event. With no death,
  ADT is the last-alive date (LSTALVDT) when that date falls after
  STARTDT, and STARTDT itself otherwise, including on a tie and when no
  last-alive date is recorded.
- AVAL is the number of days from STARTDT through ADT, counting both
  endpoints.
- CNSR is 0 when the subject has a qualifying death and 1 otherwise.
- EVNTDESC is "Death" when the subject has a qualifying death, "Alive"
  when the subject is censored at a last-alive date after randomization,
  and "Randomization" otherwise.
- CNSDTDSC has no value for a death, is "Alive During Study" when the
  subject is censored at the last-alive date, and "Randomization" when
  the subject is censored at randomization.
- SRCDOM is "ADRS" for a death and "ADSL" for censoring.
- SRCVAR is "ADT" for a death, "LSTALVDT" when the subject is censored
  at the last-alive date, and "RANDDT" otherwise.
- SRCSEQ is the ASEQ from the selected death record, and has no value
  when the record is censored.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adtte.csv.
