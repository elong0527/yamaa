Following CDISC ADaM standards, use the provided PP and ADSL datasets to
create an ADPP dataset with one record per subject per parameter per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, AVALU

New records use the parameters "ARAUC" with PARAM "Accumulation Ratio
AUCTAU" and "ARCMAX" with PARAM "Accumulation Ratio Cmax". AVISIT is the
collected VISIT. The AVAL of a new record is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
