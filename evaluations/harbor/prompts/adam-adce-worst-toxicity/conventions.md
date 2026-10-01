Following CDISC ADaM standards, use the provided CE dataset to create
an ADCE dataset with one record per clinical event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CESEQ, CETERM, ASTDT, ASEV, ASEVN, ATOXGRN, AOCCFL

ASEVN is the numeric rank of ASEV: 1 for MILD through 3 for SEVERE.
ATOXGRN is the toxicity grade, equal to ASEVN, so a moderate event reads
grade 2. AOCCFL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adce.csv.
