Following CDISC SDTM standards, use the provided ODM and LBRANGE datasets
to create an LB dataset with one record per collected laboratory result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, SEX, LBNAM, LBSTRESN, LBORRESU,
LBSTNRLO, LBSTNRHI, LBNRIND

LBTESTCD is the collected test code, SEX the recorded sex, LBNAM the
collecting lab, and LBSTRESN the numeric result in standard units, which
has no value when the result was not collected. LBSEQ is the panel repeat
key.

Each result is judged against the reference entry for its lab, test code,
and sex whose age band holds the subject's age at collection and which was
in effect on the collection date: a result collected on the first day of a
new range uses the new limits, and an age on a band boundary takes the
band that starts at that age. LBORRESU, LBSTNRLO, and LBSTNRHI are the
unit and the lower and upper limits from that entry; all three have no
value when no entry matches.

LBNRIND is LOW when the result is below the lower limit, HIGH when it is
above the upper limit, and NORMAL otherwise; it has no value when the
result or either limit is missing. A record whose result is missing still
carries its unit and limits.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
