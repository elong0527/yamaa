Following CDISC SDTM standards, use the provided DOSING_LOG dataset to create
an EC dataset with one record per dosing-log day.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ECSEQ, ECTRT, ECMOOD, ECOCCUR, ECREASND, ECDOSE,
ECDOSU, ECDOSFRM, ECDOSFRQ, ECROUTE, ECSTDTC, ECENDTC, ECADJ

- DOMAIN is "EC" and ECTRT is "Study Drug" on every record.
- ECMOOD is "PERFORMED" on every record.
- ECOCCUR is "Y" or "N".
- ECDOSFRM is "TABLET" and ECROUTE is "ORAL" on every record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ec.csv.
