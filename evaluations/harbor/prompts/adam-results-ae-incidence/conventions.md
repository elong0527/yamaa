Following CDISC ADaM standards, use the provided ADAE and ADSL datasets to
create a RESULTS dataset with one record per planned treatment group and
operation.

The output dataset should contain the following columns in this order:
STUDYID, ANALYSISID, ANALYSISSETID, DATASET, VARIABLE, ANALYSISSET, DATASUBSET,
GROUPINGID, GROUPORDER, GROUPLEVEL, METHODID, OPERATIONID, RESULT

ANALYSISID is "AEINC", ANALYSISSETID is "SAFETY", DATASET is "ADAE",
VARIABLE is "USUBJID" and METHODID is "AEINCIDENCE". ANALYSISSET is
"SAFFL = 'Y'" and DATASUBSET is "TRTEMFL = 'Y'". GROUPINGID is "TRT01P".
OPERATIONID is "COUNT" or "PERCENT". Neither result is rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/results.csv.
