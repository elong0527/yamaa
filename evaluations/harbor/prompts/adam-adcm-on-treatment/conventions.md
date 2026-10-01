Following CDISC ADaM standards, use the provided CM and ADSL datasets to
create an ADCM dataset with one record per medication.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, ASTDT, AENDT, TRTSDT, TRTEDT, ONTRTFL

ONTRTFL is Y or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
