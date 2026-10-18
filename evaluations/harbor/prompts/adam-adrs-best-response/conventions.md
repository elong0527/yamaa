Following CDISC ADaM standards, use the provided ADRS_SELECTION and ADSL
datasets to create an ADRS dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, RANDDT, AVALC, AVAL, ADT

- PARAMCD is "BOR" and PARAM is "Best Overall Response by
  Investigator" on every record.
- AVALC is "CR" for complete response, "PR" for partial response,
  "SD" for stable disease, "NON-CR/NON-PD" for neither complete
  response nor progressive disease, "PD" for progressive disease,
  or "NE" for not evaluable, or has no value.
- AVAL is 1 for "CR", 2 for "PR", 3 for "SD", 4 for
  "NON-CR/NON-PD", 5 for "PD", and 6 for "NE", or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
