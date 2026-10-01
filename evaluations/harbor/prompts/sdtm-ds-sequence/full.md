Following CDISC SDTM standards, use the provided DS_RAW dataset to
create a DS dataset with one record per collected disposition record.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, DSSEQ, DSDECOD, DSCAT, DSDTC

USUBJID is the collected subject number. DSDECOD is the recorded
outcome: COMPLETED, RANDOMIZED, ADVERSE EVENT, or SCREEN FAILURE. DSCAT
is PROTOCOL MILESTONE when the outcome is RANDOMIZED, and DISPOSITION
EVENT for any other outcome. DSDTC is the collection date; a partial
entry such as a year and month is completed to the 15th of that month,
and a year alone is completed to June 15th. It has no value when the
date is missing or cannot be read. DSSEQ numbers the subject's records
from the earliest completed collection date, so a partial entry takes
its position at its completed date, and a record with no date is
numbered after dated records. The form label feeds no output column.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ds.csv.
