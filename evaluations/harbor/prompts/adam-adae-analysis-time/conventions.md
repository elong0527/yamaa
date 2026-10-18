Following CDISC ADaM standards, use the provided AE dataset to create an
ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, ASTDT, ASTTM, ASTTMF

ASTTMF is H, M or S, or has no value. ASTDTM is written as
YYYY-MM-DDThh:mm:ss, and ASTTM is a whole number of seconds since
midnight.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
