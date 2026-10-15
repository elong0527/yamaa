Following CDISC ADaM standards, use the provided DM and EX datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AP01SDT, AP01EDT, AP02SDT, AP02EDT

The last period a subject entered ends 30 days after the last exposure
end date, and AP01EDT is the day before AP02SDT.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
