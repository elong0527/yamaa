Following CDISC SDTM standards, use the provided ODM dataset to create an
LB dataset with one record per collected glucose and creatinine result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU,
LBSTRESN, LBSTRESU, LBDTC

LBTESTCD is GLUC for glucose rows and CREAT for creatinine rows; LBTEST is
Glucose when the test code is GLUC and Creatinine when it is CREAT.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
