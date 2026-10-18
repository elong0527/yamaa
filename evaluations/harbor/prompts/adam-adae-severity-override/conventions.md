Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, ASEV, ASEVN

ASEV is "MILD", "MODERATE", "SEVERE", or "LIFE-THREATENING", or has no value.
ASEVN is the numeric rank of ASEV: 1 for "MILD", 2 for "MODERATE", 3
for "SEVERE", and 4 for "LIFE-THREATENING". ASEVN has no value when ASEV has no
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
