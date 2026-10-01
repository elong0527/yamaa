Following CDISC SDTM standards, use the provided ODM dataset to create an
LB dataset with one record per collected glucose and creatinine result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU,
LBSTRESN, LBSTRESU, LBDTC

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

LBSEQ numbers the records within a subject by collection date, then test
code. LBTESTCD is GLUC for glucose rows and CREAT for creatinine rows;
LBTEST is Glucose when the test code is GLUC and Creatinine when it is
CREAT.

LBORRES is the collected entry, kept exactly as reported. LBORRESU and
LBSTRESU are mg/dL for every record. LBSTRESN is the numeric form of the
collected entry, and has no value when the entry is not a number. LBDTC is
the collection date from the same visit group.

A test with no collected entry produces no record, so the records are only
results actually reported.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
