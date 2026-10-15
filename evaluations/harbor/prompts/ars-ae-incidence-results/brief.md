Following CDISC Analysis Results Standard v1.0, use the provided ADSL and
ADAE datasets to create a results dataset with one record per planned
treatment group represented in the safety population per operation: the
subject count and percentage with at least one treatment-emergent event.

The output dataset should contain the following columns in this order:
STUDYID, ANALYSISID, ANALYSISSETID, DATASET, VARIABLE, ANALYSISSET,
DATASUBSET, GROUPINGID, GROUPORDER, GROUPLEVEL, METHODID, OPERATIONID, RESULT

Read the source datasets from /app/input and save the completed dataset as
/app/output/results.csv.
