Following CDISC SDTM standards, use the provided ODM dataset to create an
LB dataset with one record per collected calcium and creatinine result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU,
LBSTRESN, LBSTRESU, LBSTAT, LBDTC

LBTESTCD is CA for calcium rows and CREAT for creatinine rows; LBTEST is
Calcium when the test code is CA and Creatinine when it is CREAT.

LBSTAT is NOT DONE or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
