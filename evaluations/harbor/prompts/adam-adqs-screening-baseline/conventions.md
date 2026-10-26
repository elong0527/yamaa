Following CDISC ADaM standards, use the provided ADSL and QS datasets to create
an ADQS dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, VISITNUM, VISIT, ADT, TRTSDT, AVAL, ABLFL,
SCBLFL, BASE, CHG

PARAMCD and PARAM are the collected QSTESTCD and QSTEST, unchanged. ABLFL is
"Y" or has no value. SCBLFL (Screening Used as Baseline Flag) is "Y" or has
no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adqs.csv.
