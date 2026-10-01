Following CDISC ADaM standards, use the provided AE and ADSL datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, ASTDT, ASTDTC, ASTDTF,
TRTSDT, TRTEMFL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
