Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, ANRLO, ANRHI

ANRLO and ANRHI are the lower and upper limits of the parameter's 95%
reference range. Neither limit is rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
