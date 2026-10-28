Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVISITN, AVISIT, AVAL

PARAM is 'Alanine Aminotransferase (U/L)' for ALT and
'Aspartate Aminotransferase (U/L)' for AST. AVISIT is 'Baseline' for visit
1, 'Week 2' for visit 2 and 'Unscheduled' for any other visit. AVAL is not
rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
