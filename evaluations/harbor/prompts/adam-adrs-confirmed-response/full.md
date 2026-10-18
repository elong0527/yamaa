Following CDISC ADaM standards, use the provided RS dataset to create an ADRS
dataset with one record per tumor response assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, RSSEQ, ADT, AVALC, CONFIRMED

PARAMCD is "CONFRESP" on every record. ADT is the collection date and
AVALC is the standardized result, which has no value when no result was
collected. Each assessment is compared with the next assessment of the
same subject in analysis date order. Assessments sharing a date are
ordered by sequence number: the higher-numbered one is the next
assessment for the lower-numbered one, zero days later, so it never
confirms it. CONFIRMED is "Y" when AVALC is "PD", which needs no
confirmation. CONFIRMED is "Y" when AVALC is "PR" or "CR" and the next
assessment has a result of "PR" or "CR" at least 28 days later.
CONFIRMED is "N" otherwise: when the assessment has no result, when the
next assessment is less than 28 days later, when its result is neither
"PR" nor "CR", or when there is no next assessment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
