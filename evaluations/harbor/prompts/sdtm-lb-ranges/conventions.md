Following CDISC SDTM standards, use the provided LB_RAW, LBRANGE, and DM
datasets to create an LB dataset with one record per collected test
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBSTRESU, LBSTNRLO,
LBSTNRHI, LBNRIND

LBSEQ is the sequence number as collected. LBSTRESN is the numeric result in
standard units. LBSTNRLO and LBSTNRHI are in standard units.

LBNRIND is LOW, HIGH, or NORMAL, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
