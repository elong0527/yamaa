Following CDISC ADaM standards, use the provided DM, EX and DS
datasets to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EOP01STT, DCP01RS, EOP02STT, DCP02RS

Period 1 is the TREATMENT 1 epoch and period 2 is the TREATMENT 2
epoch. A subject received a dose in a period when the subject has an
exposure record in that period's epoch.

EOP01STT is based on the subject's period 1 disposition event, the
record whose category is DISPOSITION EVENT and whose epoch is
TREATMENT 1: COMPLETED when its coded term is COMPLETED,
DISCONTINUED for any other coded term, ONGOING when the subject
received a dose in period 1 but has no such event yet, and it has no
value when the subject never received a dose in period 1. DCP01RS is
the coded term of that event when EOP01STT is DISCONTINUED, never
the reported term, and has no value otherwise.

EOP02STT and DCP02RS are the same for period 2, read from the
TREATMENT 2 epoch.

The epoch alone places an event in a period; its date, complete or
partial, plays no part. Records in other categories, and events in
other epochs, never set a period's status.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
