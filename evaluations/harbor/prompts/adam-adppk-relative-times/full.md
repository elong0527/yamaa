Following CDISC ADaM standards, use the provided EX and PC datasets to create
an ADPPK dataset with one record per subject per dose or PK concentration
record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADTM, EVID, AMT, FANLDTM, AFRLT, APRLT, DOSEA

Each EX administration with EXDOSE above zero gives a dose record, and each
PC sample gives a concentration record. An administration with a dose of
zero gives no record and is never a first or a previous dose. Date/times
are clock readings taken as written, with no time zone.

- ASEQ: the record's number within the subject, from 1, over dose and
  concentration records together in the order of ADTM, with a sample
  before a dose at the same date/time.
- ADTM: EXSTDTC on a dose record and PCDTC on a concentration record.
- EVID: 1 on a dose record and 0 on a concentration record.
- AMT: EXDOSE on a dose record; it has no value on a concentration record.
- FANLDTM: the date/time of the subject's earliest dose above zero, on
  every record of the subject.
- AFRLT: the hours from FANLDTM to ADTM, not rounded and negative before
  the first dose.
- APRLT: 0 on a dose record. On a concentration record, the hours from the
  previous dose to ADTM, not rounded, where the previous dose is the
  subject's latest dose before the sample. A sample taken at the date/time
  of a dose was taken before that dose, so it refers to the dose before
  it. A sample with no dose before it has the value of AFRLT.
- DOSEA: EXDOSE on a dose record. On a concentration record, the amount of
  the previous dose, or of the subject's first dose for a sample with no
  dose before it.

ADTM and FANLDTM are written as YYYY-MM-DDThh:mm:ss, with the seconds
always shown.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
