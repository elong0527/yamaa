Following CDISC ADaM standards, use the provided DM and EX datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TR01SDT, TR01EDT, TR02SDT, TR02EDT, TRT01A, TRT02A, WASHDUR

- TRT01A is either "VITAMIN D3" or "PLACEBO", or has no value.
- TRT02A is either "VITAMIN D3" or "PLACEBO", or has no value.
- WASHDUR is the number of days strictly between TR01EDT and TR02SDT,
  counting neither endpoint.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
