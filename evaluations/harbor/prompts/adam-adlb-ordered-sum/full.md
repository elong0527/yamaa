Following CDISC ADaM standards, use the provided LB dataset to create
an ADLB dataset with one record per subject per visit per parameter,
including one total record per subject and visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, DTYPE

Keep every collected component record with its test code and name,
visit, and result; DTYPE has no value on these records. A component with
no collected result is still kept, and its AVAL has no value.

Add one total record per subject and visit with PARAMCD "TOTAL", PARAM
"Total of Components", and DTYPE "CALCULATION". Its AVAL is the sum of
the collected component results at that subject and visit; it has no
value, rather than zero, when none of them has a collected result.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
