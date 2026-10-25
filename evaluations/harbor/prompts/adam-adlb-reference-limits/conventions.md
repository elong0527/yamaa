Following CDISC ADaM standards, use the provided LB dataset to create an ADLB
dataset with one record per subject per laboratory test.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVAL, ANRLO, ANRHI

ANRLO and ANRHI are the limits of a 95% normal-theory reference range and
are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
