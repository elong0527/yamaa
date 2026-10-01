Following CDISC ADaM standards, use the provided ADRS_RAW dataset to
create an ADRS dataset with one record per collected overall response
assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADT, RANDDY, AVALC, BORCAT, BORPRI, BORSEQ

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
