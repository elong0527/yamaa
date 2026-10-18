Following CDISC SDTM standards, use the provided DM, LB_MAPPING, and ODM
datasets to create an LB dataset with one record per laboratory result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSPEC, LBORRES, LBDTC, LBSTAT,
LBLOBXFL

DOMAIN is "LB". LBLOBXFL is "Y" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
