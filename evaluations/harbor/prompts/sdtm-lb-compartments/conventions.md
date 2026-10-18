Following CDISC SDTM standards, use the provided BX_RAW dataset to create an LB
dataset with one record per skin compartment per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBSPEC, LBLOC, LBORRES,
LBORRESU, LBSTRESN, LBSTAT

DOMAIN is "LB" on every record. LBTESTCD is "IL13" and LBTEST is
"Interleukin 13" on every record. LBSPEC is "SKIN" on every record.
LBLOC is "LESIONAL" or "NON-LESIONAL". LBSTAT is "NOT DONE" or has
no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
