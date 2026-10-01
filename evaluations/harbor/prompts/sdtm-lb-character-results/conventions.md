Following CDISC SDTM standards, use the provided ODM dataset to create an
LB dataset with one record per collected laboratory result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU,
LBSTRESC, LBSTRESN, LBSTRESU, LBNRIND, LBDTC

The test code and name are PROT and Protein, GLUC and Glucose, KETON and
Ketones, CREAT and Creatinine, or CK and Creatine Kinase.

LBSTRESC folds known dipstick spellings to NEGATIVE, TRACE, 1+, or 2+. LBNRIND
is LOW, HIGH, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
