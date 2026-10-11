Following CDISC ADaM standards, use the provided AE, FA and ADSL datasets
to create an ADAE dataset with one record per analysis severity period.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, AESEQ, AEDECOD, AESTDTC, AEENDTC, ASTDTM, AENDTM,
TRTSDTM, AESEV, AETOXGR, ASEV, ATOXGRN, TRTEMFL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
