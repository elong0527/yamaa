Following CDISC ADaM standards, use the provided DM and EX datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AP01SDT, AP01EDT, AP02SDT, AP02EDT

Every DM subject keeps a record. AP01SDT is the earliest EXSTDTC among
the subject's EX records in the TREATMENT 1 epoch, skipping a record with
no start date, and AP02SDT the same for the TREATMENT 2 epoch; each is
empty when no such record has a start date, so AP02SDT is empty when the
subject never entered period two. The last exposure end date is the
latest EXENDTC over all of the subject's EX records, including one with
no start date. AP02EDT is 30 days after the last exposure end date, and
empty when the subject never entered period two. AP01EDT is the day
before AP02SDT when the subject entered period two; otherwise it is 30
days after the last exposure end date. AP01EDT is empty when the subject
has no exposure end date. Dates move by whole calendar days across month
ends, year ends and 29 February, and a subject with no EX records has all
four dates empty. Dates are written as YYYY-MM-DD.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
