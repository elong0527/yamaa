Following CDISC ADaM standards, use the provided LB dataset to create
an ADLBC dataset with one record per subject per parameter per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, PREV_AVAL, CHG, PREV2

Build the albumin change parameter "_ALB" from the albumin lab
results; any other test produces no rows.

AVISITN is the analysis visit number, taken from the lab visit number.
AVAL is the numeric albumin result for the visit.

PREV_AVAL is the value of the visit just before in visit order, for
the same subject. It has no value on the subject's first visit.

CHG is the change since that visit, not the change from baseline; it
has no value whenever PREV_AVAL has none. PREV2 is the previous
visit's change, read after that change is complete; it has no value
until two earlier visits carry results.

A record with no numeric result gets no row, so the change and its lag
compare against the latest earlier visit with a result. A zero change
is a real value (0), distinct from no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlbc.csv.
