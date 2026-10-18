Following CDISC ADaM standards, use the provided DM and EX datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TR01SDT, TR01EDT, TR02SDT, TR02EDT, TRT01A, TRT02A, WASHDUR

- TR01SDT is the earliest EXSTDTC among the subject's exposure records in
  the "TREATMENT 1" epoch with a recorded start date, and has no value when
  the subject has no such record.
- TR01EDT is the latest EXENDTC among the subject's exposure records in
  the "TREATMENT 1" epoch with a recorded end date, and has no value when
  the subject has no such record.
- TR02SDT is the earliest EXSTDTC among the subject's exposure records in
  the "TREATMENT 2" epoch with a recorded start date, and has no value when
  the subject has no such record.
- TR02EDT is the latest EXENDTC among the subject's exposure records in
  the "TREATMENT 2" epoch with a recorded end date, and has no value when
  the subject has no such record.
- TRT01A is the EXTRT of the subject's earliest exposure record in the
  "TREATMENT 1" epoch, earliest by EXSTDTC, with the lower EXSEQ breaking
  ties between records that start on the same day and a record with no
  start date counting as later than any record with one; TRT01A is either
  "VITAMIN D3" or "PLACEBO", and has no value when the subject has no
  exposure record in the "TREATMENT 1" epoch.
- TRT02A is the EXTRT of the subject's earliest exposure record in the
  "TREATMENT 2" epoch, chosen the same way as TRT01A; TRT02A is either
  "VITAMIN D3" or "PLACEBO", and has no value when the subject has no
  exposure record in the "TREATMENT 2" epoch.
- WASHDUR is the number of days strictly between TR01EDT and TR02SDT,
  counting neither endpoint, so periods that touch give zero, and has no
  value when either date has no value.
- A subject with no exposure record in the "TREATMENT 2" epoch has no
  value for TR02SDT, TR02EDT, TRT02A, and WASHDUR.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
