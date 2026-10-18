Following CDISC ADaM standards, use the provided VS dataset to create an ADVS
dataset with one record per subject per parameter per analysis visit.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVISITN, AVAL

AVAL is the collected value when the record has one; otherwise it is the
value from the closest earlier visit, in visit-number order, that has one,
for the same subject and parameter. A gap before the first collected value
has no value. A collected value of zero is carried like any other value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
