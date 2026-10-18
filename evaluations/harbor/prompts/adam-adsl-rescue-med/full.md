Following CDISC ADaM standards, use the provided CM and DM datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, RESCTRT

RESCTRT is the treatment name (CMTRT) of the subject's first rescue
medication: a CM record whose category (CMCAT) is "RESCUE MEDICATION",
chosen by the earliest start date (CMSTDTC), with the smaller CMSEQ
breaking a tie between records with the same start date. RESCTRT has no
value when the subject took no rescue medication, whether the subject
took other medications or has no medication records at all, and it is
never filled with placeholder text. A rescue record with no recorded
start date still counts. A duplicated rescue record does not change the
answer. A start date recorded with only a year and month compares as
written, so it falls before any full date in the same month.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
