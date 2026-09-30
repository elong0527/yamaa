Following CDISC ADaM standards, use the provided LB dataset to create
an ADLBC dataset with one record per subject per parameter per
analysis visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISITN, AVAL, PREV_AVAL, CHG

Build the change parameters "_ALB" and "_BILI" from the albumin and
bilirubin lab results; any other test produces no rows.

AVISITN is the analysis visit number, taken from the lab visit number.
AVAL is the numeric lab result for the visit.

PREV_AVAL is the value of the visit just before in visit order, for
the same subject and parameter, so an _ALB row never reads a _BILI
value. It has no value on the subject's first visit for the
parameter.

CHG is the change since that visit, not the change from baseline; it
has no value whenever PREV_AVAL has none. A record with no numeric
result gets no row, so the previous value and the change skip that
visit and compare with the latest earlier visit that has a result.
Equal consecutive values give CHG 0, and a single-visit subject leaves
both empty.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlbc.csv.
