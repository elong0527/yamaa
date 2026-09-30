Following CDISC ADaM standards, use the provided RS dataset to create
an ADRS dataset with one record per response assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, RSSEQ, ADT, AVALC, CONFIRMED

Use PARAMCD "CONFRESP", where ADT is the collection date and AVALC
carries the standardized result.

CONFIRMED is Y when the result is progressive disease (PD), which
needs no confirmation, or when a partial (PR) or complete (CR)
response is followed at least 28 days later by another partial or
complete response. It is N when the current assessment has no result,
or when the next response is too early, is not a response, or does not
exist. Each assessment is compared with the next one in analysis date
order within a subject.

Assessments sharing a date are ordered by sequence number: the
higher-numbered one is the next assessment for the lower-numbered one,
zero days later, so it never confirms it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
