Following CDISC SDTM standards, use the provided DM, LBRANGE, and LB_RAW
datasets to create an LB dataset with one record per collected test
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBSTRESU, LBSTNRLO,
LBSTNRHI, LBNRIND

DOMAIN is "LB". The record's test code and the subject's sex in DM
together select the reference entry in LBRANGE. LBSTRESU is that
entry's unit, LBSTNRLO is its lower limit, and LBSTNRHI is its upper
limit. When no entry matches the record's test code and the subject's
sex, LBSTRESU, LBSTNRLO, and LBSTNRHI have no value. LBNRIND is "LOW"
when the result is below the lower limit, "HIGH" when it is above the
upper limit, and "NORMAL" otherwise. LBNRIND has no value when the
result or either limit has no value. A record whose result was not
collected still carries its unit and its lower and upper limits.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
