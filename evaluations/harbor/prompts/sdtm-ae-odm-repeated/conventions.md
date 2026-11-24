Following CDISC SDTM standards, use the provided ODM dataset to create an AE
dataset with one record per reported event.

The output dataset should contain the following columns in this order:
STUDYID, DOMAIN, USUBJID, AESEQ, AETERM, AESTDTC, AEENDTC, AESEV, AESER

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
