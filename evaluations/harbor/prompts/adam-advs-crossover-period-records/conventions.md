Following CDISC ADaM standards, use the provided ADSL and VS datasets to create
an ADVS dataset with one record per vital-signs record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VSSEQ, PARAMCD, AVAL, ADT, ADTM, APERIOD, TRTA

TRTA is the actual treatment of the record's period: TRT01A in period 1 and
TRT02A in period 2, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
