Following CDISC ADaM standards, use the provided CE dataset to create
an ADCE dataset with one record per clinical event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CESEQ, CETERM, ASTDT, ASEV, ASEVN, ATOXGRN, AOCCFL

Read the source datasets from /app/input and save the completed dataset as
/app/output/adce.csv.
