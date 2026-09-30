Following CDISC ADaM standards, use the provided VS dataset to create an
ADVS dataset with one record per subject per parameter per planned
visit.

The output dataset should contain the following columns in this order:
USUBJID, PARAMCD, AVISITN, AVAL

AVISITN identifies the planned analysis visit. AVAL is the collected
value when there is one; otherwise it is the value from the closest
earlier visit, in visit-number order, that has one, for the same
subject and parameter. A gap before the first collected value has no
value, and zero is a collected value that is carried like any other.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
