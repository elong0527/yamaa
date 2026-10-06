Following CDISC ADaM standards, use the provided VS dataset to create
an ADFA dataset with one record per collected temperature record in
the reactogenicity category.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ASEQ, PARAMCD, PARAM, AVAL, AVALC, ADT,
SRCDOM, SRCVAR, SRCSEQ

AVALC is Y at 38 degrees Celsius or higher and N below it, blank when
the result is missing or not in Celsius. AVAL is 1 for Y and 0 for N.
ASEQ numbers the subject's records from 1 in assessment-date order,
ties broken by the collected sequence number.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adfa.csv.
