Following CDISC ADaM standards, use the provided IS dataset to create an ADIS
dataset with one record per subject per parameter per visit.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, AVISIT, AVAL

- PARAMCD is "NABLOG10" on every record, the base-10 logarithm of the
  neutralizing antibody titer.
- AVISIT is the collected visit name (VISIT).
- AVAL is the base-10 logarithm of the titer (ISSTRESN) and is not rounded.
  AVAL has no value when the titer has no value.
- A titer below the assay's lower limit of quantitation (ISLLOQ), a titer
  of zero among them, gives no record. A record whose titer has no value
  stays in the output.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adis.csv.
