Following CDISC ADaM standards, use the provided ADSL and VS datasets to create
an ADVS dataset with one record per vital-signs record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VSSEQ, PARAMCD, AVAL, ADT, ADTM, APERIOD, TRTA

APERIOD is 1, 2, or has no value. A period includes its start and end. A
record is compared by time with a period limit when both have a time, and by
date otherwise. TRTA is the actual treatment of the record's period: TRT01A
in period 1 and TRT02A in period 2, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
