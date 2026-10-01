Following CDISC SDTM standards, use the provided AE_RAW dataset to
create an AE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESER, AESDTH, AESLIFE, AESHOSP,
AESDISAB, AESCONG, AESMIE

AESER is Y when any seriousness criterion is Y: death, life threat,
required or prolonged hospitalization, disability, congenital anomaly,
or another medically important event. It is N when no criterion is Y,
including when every criterion is empty. The six criterion flags are
kept as collected, so an empty flag stays empty rather than becoming N.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
