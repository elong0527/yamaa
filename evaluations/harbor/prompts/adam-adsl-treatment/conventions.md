Following CDISC ADaM standards, use the provided DM and EX datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, TRTSDT, TRTSDTM, TRTSTMF, TRTEDT, TRTEDTM, TRTETMF,
TRTDURD, SAFFL

TRT01A is "VITAMIN D3", "PLACEBO", the subject's actual arm (ACTARM), or
"NOT TREATED", in upper case. TRTSDTM is a date and time with the date and
time joined by a T, and a start collected without a time given the time
00:00:00. TRTEDTM is a date and time with the date and time joined by a T,
and an end collected without a time given the time 23:59:59. TRTSTMF is "H",
"M" or "S", or has no value. TRTETMF is "H", "M" or "S", or has no value.
TRTDURD is the number of days from TRTSDT to TRTEDT counting both the first
and the last day, so a single treatment day gives one. SAFFL is "Y" or "N".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
