Following CDISC ADaM standards, use the provided ADSL and ADRSSEL
datasets to create an ADRS dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, RANDDT, AVALC, AVAL, ADT

Use PARAMCD "BOR" and PARAM Best Overall Response by Investigator.
RANDDT is the subject's randomization date.

AVALC is the best overall response, taken from the subject's winning
selection record: complete response (CR), partial response (PR),
stable disease (SD), neither complete response nor progressive disease
(NON-CR/NON-PD), progressive disease (PD), or not evaluable (NE). It
has no value when the subject has no winning record.

AVAL ranks that response as 1 (complete response), 2 (partial
response), 3 (stable disease), 4 (neither complete response nor
progressive disease), 5 (progressive disease), or 6 (not evaluable);
it has no value when AVALC has none.

ADT is the analysis date supporting the response, taken from that
record's date; it has no value whenever AVALC has none. The supporting
date is never before the randomization date; it may fall exactly on
it. Every subject keeps its record with the randomization date even
when response, rank, and date are all empty.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
