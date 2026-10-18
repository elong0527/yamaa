Following CDISC ADaM standards, use the provided DM and EX datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01A, TRTSDT, TRTSDTM, TRTSTMF, TRTEDT, TRTEDTM, TRTETMF,
TRTDURD, SAFFL

TRT01A is the treatment actually received, in upper case. TRT01A is the
treatment named by the subject's earliest qualifying exposure record,
where a qualifying record names "VITAMIN D3" or "PLACEBO" as its
treatment and carries a start date/time, and records with the same start
are ordered by the lower EXSEQ. When no exposure record qualifies, TRT01A
is the subject's actual arm (ACTARM). When neither an exposure record nor
the actual arm names a treatment, TRT01A is "NOT TREATED". TRTSDT is the
date of the earliest qualifying exposure start, and TRTSDTM is that start
as a date and time, with the date and time joined by a T and a start
collected without a time given the time 00:00:00. TRTSDT and TRTSDTM have
no value when the subject has no qualifying exposure record. TRTSTMF is
"H" when the first exposure start was collected without a time, and has
no value otherwise. TRTEDT is the date of the latest exposure end among
the subject's records naming "VITAMIN D3" or "PLACEBO", whether or not
that record carries a start, and TRTEDTM is that end as a date and time,
with the date and time joined by a T and an end collected without a time
given the time 23:59:59. TRTEDT and TRTEDTM have no value when no record
naming "VITAMIN D3" or "PLACEBO" has an end. TRTETMF is "H" when the last
exposure end was collected without a time, and has no value otherwise.
TRTDURD is the number of days from TRTSDT to TRTEDT counting both the
first and the last day, so a single treatment day gives one, and it has
no value when either date has no value. SAFFL is "Y" when the subject has
a treatment start date (TRTSDT) and "N" otherwise, and it always has a
value. The start and end dates and times and the duration are either all
present or all have no value together.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
