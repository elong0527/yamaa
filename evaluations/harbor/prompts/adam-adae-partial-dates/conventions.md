Following CDISC ADaM standards, use the provided ADSL and AE datasets to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, ASTDT, ASTDTC, ASTDTF, TRTSDT,
TRTEMFL

ASTDTC is ASTDT written as text in the form "YYYY-MM-DD" or has no
value. ASTDTF is "D" or has no value. TRTEMFL is "Y" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
