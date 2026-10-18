Following CDISC SDTM standards, use the provided DOSING_LOG dataset to create
an EC dataset with one record per dosing-log day.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ECSEQ, ECTRT, ECMOOD, ECOCCUR, ECREASND, ECDOSE,
ECDOSU, ECDOSFRM, ECDOSFRQ, ECROUTE, ECSTDTC, ECENDTC, ECADJ

- DOMAIN is "EC" and ECTRT is "Study Drug" on every record.
- ECMOOD is "PERFORMED" on every record.
- ECSEQ numbers each subject's dosing days from 1 in date order.
- ECOCCUR is "Y" when the subject took the day's dose and "N" when the
  dose was not taken.
- ECREASND is the reason the log records for a dose that was not taken
  and has no value when the dose was taken.
- ECDOSE is the number of tablets the subject took that day and has no
  value on a day the dose was not taken.
- ECDOSU is "tablet", ECDOSFRM is "TABLET", ECDOSFRQ is "QD", and
  ECROUTE is "ORAL" on every record, including a day the dose was not
  taken.
- ECSTDTC and ECENDTC are both the dosing day.
- ECADJ is the adjustment reason the log records for the day and has no
  value when the day records none.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ec.csv.
