Following CDISC SDTM standards, use the provided LB_RAW, LBRANGE, and DM
datasets to create an LB dataset with one record per collected test
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBSTRESU, LBSTNRLO,
LBSTNRHI, LBNRIND

LBSEQ is the sequence number as collected. LBTESTCD is the test code as
collected; together with sex from DM it selects the reference entry.
LBSTRESN is the numeric result in standard units as collected, and has no
value when the result was not collected.
LBSTRESU, LBSTNRLO, and LBSTNRHI are the reference unit and the lower and
upper limits for that test-and-sex combination, in standard units.

LBNRIND is LOW when the result is below the lower limit, HIGH when it is
above the upper limit, and NORMAL otherwise; it has no value when the
result or either reference limit is missing.

A combination with no reference entry leaves the unit and both limits
with no value, and its range indicator has no value. A record whose result
is missing still carries its unit and limits.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
