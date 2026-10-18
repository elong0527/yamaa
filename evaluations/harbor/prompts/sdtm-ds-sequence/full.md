Following CDISC SDTM standards, use the provided DS_RAW dataset to create a DS
dataset with one record per collected disposition record.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSDECOD, DSCAT, DSDTC

DOMAIN is "DS". STUDYID is the study identifier as given, and USUBJID
is the subject identifier as given. DSDECOD is the recorded outcome:
"COMPLETED", "RANDOMIZED", "ADVERSE EVENT", or "SCREEN FAILURE".
DSCAT is "PROTOCOL MILESTONE" when the outcome is "RANDOMIZED", and
"DISPOSITION EVENT" for any other outcome. DSDTC is the collection
date: a date collected as a year and a month is completed to the 15th
of that month, and a year alone is completed to June 15th of that
year. DSDTC has no value when the date is missing or cannot be read.
DSSEQ numbers a subject's records from 1 in order of the completed
collection date, earliest first. A partial entry takes its position
at its completed date, after an earlier full date in that month. A
record with no date is numbered after the subject's dated records.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
