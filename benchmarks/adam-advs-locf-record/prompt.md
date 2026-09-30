Following CDISC ADaM standards, use the provided PLAN and OBS datasets
to create an ADVS dataset with one record per subject per parameter
per planned visit.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVISITN, AVAL, ADT, QSSEQ

AVISITN identifies the planned analysis visit. AVAL is the latest
non-missing result selected for analysis at or before that visit,
within the same subject and parameter; a result of zero counts as
collected. With no such result it has no value.

ADT and QSSEQ are the date and the source sequence number of that same
observation. A missing date on that observation has no value even when
an earlier observation has a date. The latest observation is the one
with the highest visit number, then the highest sequence number within
that visit.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
