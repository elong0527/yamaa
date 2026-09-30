Following CDISC ADaM standards, use the provided DM and CM datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RESCTRT

RESCTRT is the treatment name of the subject's first rescue
medication: a record with category RESCUE MEDICATION, picked by the
earliest start date, with the smaller sequence number breaking a tie.
It is left blank, never filled with placeholder text, when the subject
took no rescue medication, whether the subject took other medications
or has no medication records at all.

A rescue record with no recorded start date still counts, and a
duplicated record does not change the answer. A start date holding
only year and month compares as written, so it falls before any full
date in the same month.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
