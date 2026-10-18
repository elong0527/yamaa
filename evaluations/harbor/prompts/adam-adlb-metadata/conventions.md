Following CDISC ADaM standards, use the provided LB dataset to create an
ADLB dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVAL, AVALC, AVALU

PARAM is 'Glucose (mg/dL)' for GLUC, 'Creatinine (umol/L)' for CREAT and
'Choriogonadotropin Beta' for HCG. AVAL is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
