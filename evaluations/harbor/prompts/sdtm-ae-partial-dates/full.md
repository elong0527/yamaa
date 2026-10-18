Following CDISC SDTM standards, use the provided DM and ODM datasets to create
an AE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, AEENDTC, AESTDY, AEENDY

STUDYID is the study OID and USUBJID is the subject key, each as given in
ODM. AESEQ is the adverse event's number within the subject, the repeat
key of its AE form in ODM. AESTDTC and AEENDTC are the collected start and
end dates, their year, month, and day collected as separate items in ODM,
written as ISO 8601 text at the precision collected: a full date
(2026-03-15), a year and month (2026-03) when no day was collected, or a
year alone (2026) when no month was collected. Nothing is imputed, and a
date with no part collected has no value. AESTDY and AEENDY are the study
days of the start and end dates counted from the subject's reference start
date, RFSTDTC in DM. Only a complete date gets a study day: a partial date
has no study day even when the reference date is known, and neither date
has a study day when the subject's RFSTDTC has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
