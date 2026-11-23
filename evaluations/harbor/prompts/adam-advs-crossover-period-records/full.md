Following CDISC ADaM standards, use the provided ADSL and VS datasets to create
an ADVS dataset with one record per vital-signs record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VSSEQ, PARAMCD, AVAL, ADT, ADTM, APERIOD, TRTA

PARAMCD is the test code and AVAL is the numeric result. ADT is the
collection date and has no value when no date was collected. ADTM is the
collection date and time and has no value when only a date was collected,
since no time is supplied. Period xx starts at APxxSDT, or APxxSDTM when
a time was recorded for it, and ends at APxxEDT, or APxxEDTM when a time
was recorded for it. APERIOD is the period whose start and end the record
lies within, both included. Each limit is compared by time when both the
record and the limit have a time, and by date otherwise. So a record
exactly at a period's start or end time is in that period, a record at
10:00 on the day a period ends at 08:00 is not in it, a date-only record
on a period's first or last day is in it, and a record with a time is
placed by its date against a period with no recorded times. APERIOD has no
value for a record between the periods, before the first or after the
last, when no date was collected, and for a period with no start or end
date. When a date-only record falls on a day that ends one period and
starts the next, it is in the later period. TRTA is the actual treatment
of the record's period: TRT01A in period 1 and TRT02A in period 2. TRTA
has no value when the record has no period.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
