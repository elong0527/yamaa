Following CDISC ADaM standards, use the provided DM and EX datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, TRTSDT, TRTSDTM, TRTSTMF, TRTEDT, TRTEDTM,
TRTETMF, TRTDURD, SAFFL

TRT01A is the treatment actually received, in upper case: the
treatment of the earliest qualifying exposure (a record naming VITAMIN
D3 or PLACEBO that carries a start), with ties on the start broken by
the lower sequence number; then the actual arm when no exposure
qualifies; then the text NOT TREATED when neither source names a
treatment. An exposure record with no matching demographics record
adds no record.

TRTSDT and TRTSDTM are the date and date-time of the earliest
qualifying exposure start; both have no value when the subject has no
qualifying exposure record. A start collected as a date alone reads
midnight. TRTSTMF records which part of that start's time was supplied:
H when it was collected without a time, M when it stops at the hour, and
S when it stops at the minute; it has no value when the time was
collected to the second or there is no start.

TRTEDT and TRTEDTM are the date and date-time of the latest exposure
end among VITAMIN D3 and PLACEBO records; a record need not have a
start to name an end. Both have no value when no such record has an
end. An end collected as a date alone reads the last moment of the day.
TRTETMF records which part of that end's time was supplied, as TRTSTMF
does for the start; it has no value when the time was collected to the
second or there is no end.

TRTDURD is the number of days from TRTSDT to TRTEDT, counting both the
first and the last day, so a single treatment day gives one; it has no
value when either date has none. SAFFL is Y when the subject has a
treatment start date and N otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
