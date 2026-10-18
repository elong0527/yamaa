Following CDISC SDTM standards, use the provided ODM dataset to create an LB
dataset with one record per collected glucose or creatinine result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU, LBSTRESN,
LBSTRESU, LBDTC

DOMAIN is "LB". LBTESTCD is "GLUC" for a glucose result and "CREAT" for a
creatinine result. LBTEST is "Glucose" for "GLUC" and "Creatinine" for "CREAT".

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
