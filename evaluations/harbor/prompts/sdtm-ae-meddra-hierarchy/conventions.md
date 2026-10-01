Following CDISC SDTM standards, use the provided AE_RAW and MEDDRA
datasets to create an AE dataset with one record per collected adverse
event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AELLT, AELLTCD, AEDECOD, AEPTCD,
AEHLT, AEHLTCD, AEHLGT, AEHLGTCD, AEBODSYS, AEBODSCD, AESOC, AESOCCD

Every code is written as its digits alone, with no decimal point.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
