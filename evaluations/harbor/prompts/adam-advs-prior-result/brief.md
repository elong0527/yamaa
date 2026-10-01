Following CDISC ADaM standards, use the provided VS dataset to create an
ADVS dataset with one record per vital-signs record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VSSEQ, SERIES, AVISITN, AVALC, PREVAVALC

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
