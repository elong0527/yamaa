Following CDISC SDTM standards, use the provided DM_RAW, EX, DS, and
AE datasets to create a DM dataset with one record per enrolled
subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, RFICDTC, RFXSTDTC, RFXENDTC, RFSTDTC, RFPENDTC,
RFENDTC

RFICDTC is the consent date as collected. RFXSTDTC is the earliest
exposure start date, with no value when no exposure row carries a start
date. RFXENDTC is the latest exposure end date, with no value when no
exposure row carries an end date. RFSTDTC is the reference start date,
taken here as the first exposure date, so it has no value whenever
RFXSTDTC does. RFPENDTC is the last date the subject is known to have
participated: the latest of the last exposure end date, the latest
disposition event date, and the latest adverse event end date. A missing
one is skipped, and it has no value only when all three are. Only
disposition rows in the DISPOSITION EVENT category count. RFENDTC is
the reference end date, taken here as RFPENDTC; it has no value whenever
RFSTDTC does, so a subject who never entered the reference period has no
end date. Exposure, disposition, and adverse event rows count only for
the subject with the same study and subject identifiers.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
