Following CDISC ADaM standards, use the provided ADSL and TU datasets to create
an ADRS dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVALC, AVAL

- PARAMCD is "MDIS" and PARAM is "Measurable Disease at Baseline" on
  every record.
- AVALC is "Y" or "N".
- AVAL is 1 for "Y" and 0 for "N".

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
