Following CDISC ADaM standards, use the provided VS dataset to create an ADFA
dataset with one record per qualifying temperature record.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ASEQ, PARAMCD, PARAM, AVAL, AVALC, ADT, SRCDOM,
SRCVAR, SRCSEQ

PARAMCD is "FEVER" and PARAM is "Fever Occurrence" on every record. AVALC
is "Y" or "N", or has no value. AVAL is 1 for "Y" and 0 for "N", or has no
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adfa.csv.
