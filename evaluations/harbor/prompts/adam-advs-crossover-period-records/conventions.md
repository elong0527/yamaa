Following CDISC ADaM standards, use the provided VS and ADSL datasets
to create an ADVS dataset with one record per vital-signs record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, VSSEQ, PARAMCD, AVAL, ADT, ADTM, APERIOD, TRTA

APERIOD is 1 or 2, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/advs.csv.
