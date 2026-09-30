Following CDISC ADaM standards, use the provided DM and DS datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EOSDT, EOSDECOD, EOSREAS, EOSSTT, DCSREAS

Summarize the subject's last disposition event: the dated record with
category DISPOSITION EVENT that has the latest start date. When two
events share the latest date, the higher sequence number decides.
Records in other categories, and records without a date, never count.

EOSDT is the start date of that last event, and has no value when the
subject has no dated disposition record. EOSDECOD is its coded term
and EOSREAS its reported term, each with no value when there is no
dated disposition record.

EOSSTT is COMPLETED when the last coded term is COMPLETED,
DISCONTINUED for any other recorded term, and ONGOING when the
subject has no dated disposition record. DCSREAS repeats EOSREAS for
a DISCONTINUED subject and has no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
