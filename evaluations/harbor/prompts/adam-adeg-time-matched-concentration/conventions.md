Following CDISC ADaM standards, use the provided ADEG_RAW and ADPC datasets
to create an ADEG dataset with one record per subject per parameter per
analysis period per nominal period day per nominal time point.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTA, PARAMCD, PARAM, APERIOD, APERDAY, ATPT, ATPTN,
AVAL, AVALU, ABLFL, BASE, CHG, BASETYPE, ECGPCFL, PKCONC, CQTANLFL

The parent drug is PARAMCD "DRGX". Placebo records have TRTA "Placebo".
ECGPCFL and CQTANLFL are "Y" or have no value. PKCONC is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adeg.csv.
