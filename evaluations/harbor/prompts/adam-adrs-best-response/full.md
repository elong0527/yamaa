Following CDISC ADaM standards, use the provided ADRS_SELECTION and ADSL
datasets to create an ADRS dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, RANDDT, AVALC, AVAL, ADT

- PARAMCD is "BOR" and PARAM is "Best Overall Response by
  Investigator" on every record.
- Every subject in ADSL has a record, even when the subject has no
  ADRS_SELECTION record with BORSEQ 1.
- AVALC is the category in BORCAT on the subject's ADRS_SELECTION
  record with BORSEQ 1: "CR" for complete response, "PR" for partial
  response, "SD" for stable disease, "NON-CR/NON-PD" for neither
  complete response nor progressive disease, "PD" for progressive
  disease, and "NE" for not evaluable.
- AVALC has no value when the subject has no ADRS_SELECTION record
  with BORSEQ 1.
- AVAL is 1 when AVALC is "CR", 2 when AVALC is "PR", 3 when AVALC is
  "SD", 4 when AVALC is "NON-CR/NON-PD", 5 when AVALC is "PD", and 6
  when AVALC is "NE".
- AVAL has no value when AVALC has no value.
- ADT is the analysis date supporting the response, taken from the
  date of the subject's ADRS_SELECTION record with BORSEQ 1.
- ADT has no value whenever AVALC has no value.
- ADT is never before RANDDT, and it may fall exactly on RANDDT.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
