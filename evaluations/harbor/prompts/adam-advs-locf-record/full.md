Following CDISC ADaM standards, use the provided PLAN and VS datasets to create
an ADVS dataset with one record per subject per parameter per planned visit.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVISITN, AVAL, ADT, QSSEQ

AVAL is the latest non-missing result selected for analysis at or before
the planned visit, within the same subject and parameter. An observation
is selected for analysis when its ANL01FL is "Y". The latest observation
is the one with the highest visit number, then the highest sequence
number within that visit. A result of zero counts as a collected result.
ADT and QSSEQ are the date and the source sequence number of the same
observation that gives AVAL. A missing date stays missing even when an
earlier observation has a date. With no such observation, AVAL, ADT, and
QSSEQ have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
