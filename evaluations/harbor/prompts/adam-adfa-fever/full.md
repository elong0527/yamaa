Following CDISC ADaM standards, use the provided VS dataset to create an ADFA
dataset with one record per qualifying temperature record.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ASEQ, PARAMCD, PARAM, AVAL, AVALC, ADT, SRCDOM,
SRCVAR, SRCSEQ

A record qualifies only when it is a temperature record in the
reactogenicity category: its test code is "TEMP" and its category is
"REACTOGENICITY". Other vital signs records give no row. PARAMCD is "FEVER"
and PARAM is "Fever Occurrence" on every record. AVALC is "Y" for a
temperature of 38 degrees Celsius or higher and "N" for a lower one. AVALC
has no value when the result is missing or its unit is not "C". AVAL is 1
when AVALC is "Y", 0 when AVALC is "N", and has no value when AVALC has no
value. ASEQ numbers each subject's records 1, 2, 3, ... in order of the
assessment date, the collected sequence number breaking ties. ADT is the
calendar date of the assessment. SRCDOM is "VS", SRCVAR is "VSSTRESN", and
SRCSEQ is the collected sequence number of the source record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adfa.csv.
