Following CDISC ADaM standards, use the provided ADSL and TU datasets to create
an ADRS dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, AVALC, AVAL

- PARAMCD is "MDIS" and PARAM is "Measurable Disease at Baseline" on
  every record.
- AVALC is "Y" when the subject has at least one tumor identification
  record with TUTESTCD "TUMIDENT" at VISIT "SCREENING" whose TUSTRESC is
  "TARGET", and "N" otherwise.
- A screening record with a blank result, or with a result such as
  "NON-TARGET" or "BENIGN ABNORMALITY", is not target disease, and a
  "TARGET" result at a later visit does not count.
- AVAL is 1 when AVALC is "Y" and 0 when AVALC is "N".
- Every subject in ADSL has one record, even a subject with no screening
  tumor assessment, whose AVALC is "N" and AVAL is 0.
- A tumor record for a subject who is not in ADSL contributes to no flag
  and creates no record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adrs.csv.
