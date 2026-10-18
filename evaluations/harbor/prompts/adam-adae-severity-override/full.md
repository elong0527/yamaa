Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, ASEV, ASEVN

ASEV is the reported severity in upper case: "MILD", "MODERATE", "SEVERE", or
"LIFE-THREATENING". An approved severity correction reassigns the event of
subject CATH-01-001 with AESEQ 2 to "SEVERE", and the event of subject
CATH-01-003 with AESEQ 1 to "SEVERE". A correction also applies to an event
with no reported severity, so that event's ASEV is "SEVERE". ASEV has no
value when an event has no reported severity and no correction applies to
it. ASEVN is the numeric rank of ASEV: 1 for "MILD", 2 for "MODERATE", 3
for "SEVERE", and 4 for "LIFE-THREATENING". ASEVN reflects the corrected
ASEV, so a corrected event carries both the corrected severity and its
matching rank. ASEVN has no value when ASEV has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
