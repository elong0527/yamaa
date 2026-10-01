Following CDISC SDTM standards, use the provided TR_RAW dataset to create
a TR dataset with one record per collected tumor assessment.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, TRSEQ, TRLNKID, TRTESTCD, TRTEST, TRORRES,
TRORRESU, TRSTRESC, TRSTRESN, TRSTRESU, TRSTAT, TRMETHOD, TREVAL, VISITNUM,
TRDTC

Read the source datasets from /app/input and save the completed dataset as
/app/output/tr.csv.
