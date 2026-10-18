Following CDISC SDTM standards, use the provided DM and ODM datasets to create
an AE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, AEENDTC, AESTDY, AEENDY

AESTDTC and AEENDTC are the collected start and end dates, written as
ISO 8601 text at the precision collected: a full date (2026-03-15), a
year and month (2026-03) when no day was collected, or a year alone
(2026) when no month was collected. AESTDY and AEENDY are the study days
of the start and end dates counted from the subject's reference start
date, RFSTDTC in DM.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
