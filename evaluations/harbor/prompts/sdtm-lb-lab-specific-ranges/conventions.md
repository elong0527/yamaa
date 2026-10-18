Following CDISC SDTM standards, use the provided LBRANGE and ODM datasets to
create an LB dataset with one record per collected lab result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, SEX, LBNAM, LBSTRESN, LBORRESU,
LBSTNRLO, LBSTNRHI, LBNRIND

DOMAIN is "LB". The subject's age counts in whole years at collection,
counted from the subject's birth date to the collection date. LBNRIND is
"LOW", "HIGH", or "NORMAL", or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
