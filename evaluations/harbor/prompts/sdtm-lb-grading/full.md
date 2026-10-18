Following CDISC SDTM standards, use the provided LB_RAW dataset to create an LB
dataset with one record per collected neutrophil or hemoglobin result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSTRESN, LBTOXGR

DOMAIN is "LB". LBTOXGR is the standard toxicity grade for the
result, one of "4", "3", "2", "1", or "0", from the band set for
that test and sex; each band includes its lower limit and excludes
its upper one. Neutrophil counts use one band set whatever the
sex, with bands in 10^9/L: below 0.5 is grade "4", 0.5 to below
1.0 is grade "3", 1.0 to below 1.5 is grade "2", 1.5 to below 1.8
is grade "1", and 1.8 or above is grade "0". Hemoglobin uses one
band set per sex: for "M", below 8.0 is grade "3", 8.0 to below
10.0 is grade "2", 10.0 to below 13.5 is grade "1", and 13.5 or
above is grade "0"; for "F", below 8.0 is grade "3", 8.0 to below
10.0 is grade "2", 10.0 to below 12.0 is grade "1", and 12.0 or
above is grade "0". A result for any other test, or a hemoglobin
result with a sex other than "M" or "F", gets no record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
