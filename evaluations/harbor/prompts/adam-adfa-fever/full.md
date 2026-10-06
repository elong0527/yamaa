Following CDISC ADaM standards, use the provided VS dataset to create
an ADFA dataset with one record per collected temperature record in
the reactogenicity category.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ASEQ, PARAMCD, PARAM, AVAL, AVALC, ADT,
SRCDOM, SRCVAR, SRCSEQ

PARAMCD is FEVER and PARAM is Fever Occurrence on every record.
AVALC is Y when the collected temperature is 38 degrees Celsius or
higher and N when it is lower; it has no value when the result was
not collected or its unit is not Celsius. AVAL is 1 when AVALC is Y
and 0 when AVALC is N, and has no value when AVALC has no value.
ASEQ numbers the subject's records 1, 2, 3 and so on in
assessment-date order, with the collected sequence number breaking
ties on the same date. ADT is the calendar date of the assessment.
SRCDOM, SRCVAR and SRCSEQ trace each record to its source: the VS
domain, the VSSTRESN variable, and the collected sequence number.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adfa.csv.
