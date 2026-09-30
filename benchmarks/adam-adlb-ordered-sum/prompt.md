Following CDISC ADaM standards, use the provided LB dataset to create
an ADLB dataset with one record per subject per visit per parameter,
including one total record per subject and visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, DTYPE

Keep each collected component record with its test name and result,
with DTYPE empty. A component with no collected result contributes
nothing.

Add one total record per subject and visit with PARAMCD "TOTAL", PARAM
Total of Components, and DTYPE CALCULATION. Its value is the component
results added in the order the source records were stored; a subject
and visit with no collected results at all leaves the total empty
rather than zero.

Binary floating-point addition makes the total sensitive to that
order: records stored as 0.1, 0.2, 0.3 total 0.6000000000000001,
while the same values stored as 0.3, 0.2, 0.1 total 0.6.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
