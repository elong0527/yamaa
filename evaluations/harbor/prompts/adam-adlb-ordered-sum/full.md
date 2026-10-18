Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISIT, AVAL, DTYPE

The output holds every collected laboratory record and adds one total record
for each subject and visit. On a collected record, PARAMCD is the collected
test code, PARAM is the collected test name, and AVAL is the collected
standardized result. On the total record, PARAMCD is TOTAL and PARAM is
"Total of Components". AVAL on the total record is that subject's collected
component results for the visit, added in the order the source records were
stored. A component with no collected result contributes nothing to the
total. When a subject and visit has no collected results at all, AVAL on the
total record has no value rather than zero. AVAL is not rounded. DTYPE has
no value on the collected records and is CALCULATION on the total record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
