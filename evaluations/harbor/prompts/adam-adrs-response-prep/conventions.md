Following CDISC ADaM standards, use the provided ADRS_RAW and ADSL datasets to
create an ADRS dataset with one record per collected overall response
assessment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADT, RANDDY, AVALC, BORCAT, BORPRI, BORSEQ

BORCAT is "CR" for complete response, "PR" for partial response, "SD" for
stable disease, "NON-CR/NON-PD" for neither complete response nor progressive
disease, "PD" for progressive disease, and "NE" for not evaluable, or has no
value. BORPRI is 1 for "CR", 2 for "PR", 3 for "SD", 4 for "NON-CR/NON-PD", 5
for "PD", and 6 for "NE", or has no value. BORSEQ numbers the records of each
study and subject in BORPRI order, then by analysis date, then by assessment
sequence, starting at 1.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
