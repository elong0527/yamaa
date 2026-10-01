Following CDISC SDTM standards, use the provided ODM dataset to create an
LB dataset with one record per collected calcium and creatinine result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU,
LBSTRESN, LBSTRESU, LBSTAT, LBDTC

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

LBSEQ numbers the records within a subject by collection date, then test
code. LBTESTCD is CA for calcium rows and CREAT for creatinine rows; LBTEST
is Calcium when the test code is CA and Creatinine when it is CREAT.

LBORRESU and LBSTRESU are mg/dL or have no value. LBSTAT is NOT DONE or has
no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
