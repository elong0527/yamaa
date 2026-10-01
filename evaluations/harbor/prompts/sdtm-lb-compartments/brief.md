Following CDISC SDTM standards, use the provided BX_RAW dataset to create
an LB dataset with one record per skin compartment a subject has.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBSPEC, LBLOC, LBORRES,
LBORRESU, LBSTRESN, LBSTAT

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
