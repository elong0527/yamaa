Following CDISC ADaM standards, use the provided ADSL and LB datasets to create
an ADLB dataset with one record per subject per parameter.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, SEX, PARAMCD, AVAL, ANRLO, ANRHI, AVALVAR

ANRLO and ANRHI are the lower and upper limits of the 95% reference range,
by Hyndman and Fan's type 7 definition, and AVALVAR is the sample variance.
None of them is rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
