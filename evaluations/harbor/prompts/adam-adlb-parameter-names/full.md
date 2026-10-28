Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISITN, AVISIT, AVAL

PARAMCD is the laboratory test code. PARAM is
'Alanine Aminotransferase (U/L)' for ALT and
'Aspartate Aminotransferase (U/L)' for AST. AVISITN is the visit number.
AVISIT is 'Baseline' for visit 1, 'Week 2' for visit 2 and 'Unscheduled'
for any other visit. AVAL is the numeric standard result, not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
