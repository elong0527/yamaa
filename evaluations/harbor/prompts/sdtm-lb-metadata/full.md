Following CDISC SDTM standards, use the provided ODM dataset to create an LB
dataset with one record per collected glucose or creatinine result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBORRES, LBORRESU, LBSTRESN,
LBSTRESU, LBDTC

DOMAIN is "LB". STUDYID is the study OID and USUBJID the subject key, each
as given in ODM. Each collected glucose result and each collected creatinine
result gives one record, and a test with no collected entry gives no record.
LBTESTCD is "GLUC" for a glucose result and "CREAT" for a creatinine result.
LBTEST is "Glucose" for "GLUC" and "Creatinine" for "CREAT". LBORRES is the
collected entry, kept exactly as reported. LBORRESU is "mg/dL" for every
record. LBSTRESN is the numeric form of the collected entry and has no value
when the entry is not a number. LBSTRESU is "mg/dL" for every record. LBDTC
is the collection date collected in the same visit group. LBSEQ numbers each
subject's records from 1 by collection date, then by test code.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
