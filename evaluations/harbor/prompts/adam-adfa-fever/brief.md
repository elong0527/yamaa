Following CDISC ADaM standards, use the provided VS dataset to create
an ADFA dataset with one record per collected temperature record in
the reactogenicity category.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ASEQ, PARAMCD, PARAM, AVAL, AVALC, ADT,
SRCDOM, SRCVAR, SRCSEQ

Read the source datasets from /app/input and save the completed dataset as
/app/output/adfa.csv.
