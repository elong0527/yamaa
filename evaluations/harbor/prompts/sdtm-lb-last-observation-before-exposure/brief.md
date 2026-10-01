Following CDISC SDTM standards, use the provided ODM, DM, and LB_MAPPING
datasets to create an LB dataset with one record per collected laboratory
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSPEC, LBORRES, LBDTC, LBSTAT,
LBLOBXFL

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
