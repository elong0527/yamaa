Following CDISC ADaM standards, use the provided ADAE and ADSL datasets to
create a RESULTS dataset with one record per planned treatment group and
operation.

The output dataset should contain the following columns in this order:
STUDYID, ANALYSISID, ANALYSISSETID, DATASET, VARIABLE, ANALYSISSET, DATASUBSET,
GROUPINGID, GROUPORDER, GROUPLEVEL, METHODID, OPERATIONID, RESULT

ANALYSISID is "AEINC", ANALYSISSETID is "SAFETY", DATASET is "ADAE",
VARIABLE is "USUBJID" and METHODID is "AEINCIDENCE". ANALYSISSET is
"SAFFL = 'Y'" and DATASUBSET is "TRTEMFL = 'Y'". GROUPINGID is "TRT01P".
GROUPORDER is the planned treatment code and GROUPLEVEL the planned
treatment name of the record's planned treatment group, taken from the
subject records in ADSL. The treatment groups and both results cover only
safety subjects, the subjects with SAFFL "Y" in ADSL. A qualifying event is
an adverse event with TRTEMFL "Y", and a subject with at least one
qualifying event counts once, however many qualifying events the subject
has. Each planned treatment group with at least one safety subject has one
record with OPERATIONID "COUNT" and one with OPERATIONID "PERCENT", and a
planned treatment group with no safety subjects has no records. For
"COUNT", RESULT is the number of safety subjects in the group with at least
one qualifying event. For "PERCENT", RESULT is that number as a percentage
of all safety subjects in the group. Neither result is rounded. A planned
treatment group whose safety subjects have no qualifying events keeps both
records, with RESULT 0 in each.

Read the source datasets from /app/input and save the completed dataset as
/app/output/results.csv.
