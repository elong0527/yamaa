Following CDISC SDTM standards, use the provided ODM dataset to create
a CM dataset with one record per reported medication course.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMSTDTC, CMENDTC, CMROUTE, CMINDC

Read the source datasets from /app/input and save the completed dataset as
/app/output/cm.csv.
