Following CDISC ADaM standards, use the provided DM and EX datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AP01SDT, AP01EDT, AP02SDT, AP02EDT

AP01SDT is the earliest EXSTDTC among the subject's EX records in the
"TREATMENT 1" epoch, skipping a record with no start date, and has no value
when no such record has a start date. AP02SDT is the earliest EXSTDTC among
the subject's EX records in the "TREATMENT 2" epoch, skipping a record with
no start date, and has no value when the subject never entered period two,
that is, has no such record with a start date. AP01EDT is the day before
AP02SDT when the subject entered period two. Otherwise, AP01EDT is 30 days
after the subject's last exposure end date, so a subject who stopped after
period one is followed up within period one, and AP01EDT has no value when
the subject has no exposure end date. AP02EDT is 30 days after the subject's
last exposure end date, and has no value when the subject never entered
period two. The subject's last exposure end date is the latest EXENDTC over
all of the subject's EX records, including a record with no start date. Each
date moves by whole calendar days across month ends, year ends and
29 February. A subject with no exposure records has no value for any of the
four dates.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
