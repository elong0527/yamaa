Following CDISC ADaM standards, use the provided ADSL and AE datasets to create
an ADAE dataset with one record per AE record, the events of an untreated
subject included.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AESER, ASTDTM, TRTSDTM, TRTEDTM, EOTSTT,
TEENDDTM, TRTEMFL, AOCCFL

ASTDTM is a date and time to the second, or has no value. TEENDDTM is a date
and time or has no value, and a day is 24 hours. TRTEMFL is "Y" or has no
value. AOCCFL is "Y" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
