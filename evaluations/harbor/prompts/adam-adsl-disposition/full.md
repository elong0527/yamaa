Following CDISC ADaM standards, use the provided DM and DS datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EOSDT, EOSDECOD, EOSREAS, EOSSTT, DCSREAS,
EOTDT, EOTSTT, DCTREAS, DCSREASP

Summarize the subject's last disposition event: the dated record with
category DISPOSITION EVENT whose epoch is not TREATMENT that has the
latest start date. When two events share the latest date, the higher
sequence number decides. Records in other categories, and records
without a date, never count.

EOSDT is the start date of that last event, and has no value when the
subject has no dated disposition record. EOSDECOD is its coded term
and EOSREAS its reported term, each with no value when there is no
dated disposition record.

EOSSTT is COMPLETED when the last coded term is COMPLETED,
DISCONTINUED for any other recorded term, and ONGOING when the
subject has no dated disposition record. DCSREAS repeats EOSREAS for
a DISCONTINUED subject and has no value otherwise.

Independently, summarize the subject's treatment disposition from the
dated records with category DISPOSITION EVENT and epoch TREATMENT.
When two treatment events share the latest date, select the one with
the higher DSSEQ. EOTDT is the latest start date of those records,
with no value when the subject has none. EOTSTT is COMPLETED when the
last treatment coded term is COMPLETED, DISCONTINUED for any other
dated treatment record (including a missing coded term), and ONGOING
when the subject has no dated treatment record. DCTREAS
is the reported term on the last treatment record for a DISCONTINUED
subject and has no value otherwise; DCSREASP repeats it when the
coded treatment term is OTHER and has no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
