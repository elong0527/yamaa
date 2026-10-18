Following CDISC ADaM standards, use the provided ADSL and VS datasets to create
an ADVS dataset with one record per vital-signs record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VSSEQ, PARAMCD, AVAL, ADT, ADTM, APERIOD, TRTA

PARAMCD is the test code and AVAL is the numeric result. ADT is the
collection date and has no value when no date was collected. ADTM is the
collection date and time and has no value when only a date was collected,
since no time is supplied. APERIOD is the latest period whose start is at
or before the record. A period runs from its start up to, but not
including, the next period's start, so a record exactly at the start of
period 2 is in period 2, and a record one second earlier is in period 1. A
record with a date but no time is placed by its date against the period
start dates, so a date-only record on the day period 2 starts is in
period 2. The last period a subject started has no end. APERIOD has no
value before the first period starts or when no date was collected. TRTA
is the actual treatment of the record's period: TRT01A in period 1 and
TRT02A in period 2. TRTA has no value when the record has no period.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
