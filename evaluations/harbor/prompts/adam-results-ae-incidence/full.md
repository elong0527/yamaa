Following CDISC ADaM standards, use the provided ADSL and ADAE datasets to
create a RESULTS dataset in the style of the CDISC Analysis Results Standard
v1.0, with one record per planned treatment group represented in the safety
population per operation: the subject count and percentage with at least one
treatment-emergent event.

The output dataset should contain the following columns in this order:
STUDYID, ANALYSISID, ANALYSISSETID, DATASET, VARIABLE, ANALYSISSET,
DATASUBSET, GROUPINGID, GROUPORDER, GROUPLEVEL, METHODID, OPERATIONID, RESULT

ANALYSISID is "AEINC", ANALYSISSETID is "SAFETY", DATASET is "ADAE",
VARIABLE is "USUBJID" and METHODID is "AEINCIDENCE". ANALYSISSET is the
literal text "SAFFL = 'Y'" and DATASUBSET is "TRTEMFL = 'Y'". The safety
population contains ADSL subjects with SAFFL "Y". A qualifying event has
TRTEMFL "Y" in ADAE and belongs to a safety subject. GROUPINGID is "TRT01P";
GROUPORDER and GROUPLEVEL are the subject's TRT01PN and TRT01P. The two
OPERATIONID values are "COUNT" and "PERCENT". For COUNT, RESULT is the
number of distinct safety subjects with a qualifying event in that planned
treatment group. For PERCENT, RESULT is this count as a percentage of all
safety subjects in the same planned treatment group. Repeated events count
once per subject. Subjects outside safety contribute to neither numerator
nor denominator. A safety treatment group without qualifying events has
both result rows at 0. A group without safety subjects has no result rows.
RESULT is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/results.csv.
