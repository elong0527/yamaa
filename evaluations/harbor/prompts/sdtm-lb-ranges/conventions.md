Following CDISC SDTM standards, use the provided DM, LBRANGE, and LB_RAW
datasets to create an LB dataset with one record per collected test
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBSTRESU, LBSTNRLO,
LBSTNRHI, LBNRIND

DOMAIN is "LB". LBNRIND is "LOW", "HIGH", or "NORMAL", or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
