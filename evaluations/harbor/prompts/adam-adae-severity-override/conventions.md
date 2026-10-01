Following CDISC ADaM standards, use the provided AE dataset to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, ASEV, ASEVN

ASEV is the analysis severity in upper case, or has no value. ASEVN is the
numeric rank of ASEV: 1 for MILD through 4 for LIFE-THREATENING.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
