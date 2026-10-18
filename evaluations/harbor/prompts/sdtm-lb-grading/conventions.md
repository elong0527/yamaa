Following CDISC SDTM standards, use the provided LB_RAW dataset to create an LB
dataset with one record per collected neutrophil or hemoglobin result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBTOXGR

DOMAIN is "LB". LBTOXGR is the standard toxicity grade for the
result, one of "4", "3", "2", "1", or "0".

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
