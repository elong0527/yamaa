Following CDISC ADaM standards, use the provided EX dataset to create an ADPPK
dataset with one record per EX record with a dose (EXDOSE) above zero.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXSEQ, ADTM, ATMF, EVID, AMT, II, ADDL

Each output record is the dose record of one EX record, which covers
repeated administrations of one dose, and is identified by the EXSEQ of
that EX record. The dosing frequency (EXDOSFRQ) is "QD" once a day, "BID"
twice a day or "ONCE" a single dose. The start and the end of dosing
(EXSTDTC, EXENDTC) are each collected with or without a time of day.

- ADTM is the date/time of the first administration, in the form
  YYYY-MM-DDThh:mm:ss: the start of dosing as collected, with seconds 00.
  A start with no time of day is taken at 00:00:00.
- ATMF is "H" when the time of day of ADTM was not collected and has no
  value otherwise. Seconds are never collected in this study, so a time
  collected to the minute is not flagged.
- EVID is 1, a dose record.
- AMT is the dose given at each administration, EXDOSE.
- ADDL is the number of administrations after the first. They follow one
  every interval of the frequency, 24 hours for "QD" and 12 hours for
  "BID", for as long as the next one is not after the end of dosing; one
  exactly at the end counts. An end with no time of day covers its whole
  day. The end only bounds the administrations and is not one itself, so
  a record ending before the day's dose time has its last administration
  the day before. ADDL is 0 for a single dose and for a record with no
  end.
- II is the hours between administrations, 24 or 12, and 0 when ADDL is
  0, since a record of one dose has no interval.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
