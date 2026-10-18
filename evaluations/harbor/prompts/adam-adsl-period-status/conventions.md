Following CDISC ADaM standards, use the provided DM, DS, and EX datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EOP01STT, DCP01RS, EOP02STT, DCP02RS

Period 1 is the "TREATMENT 1" epoch and period 2 is the "TREATMENT 2"
epoch. EOP01STT is "COMPLETED", "DISCONTINUED", "ONGOING", or has no
value. DCP01RS is the coded term of the period 1 event, or has no value.
EOP02STT is "COMPLETED", "DISCONTINUED", "ONGOING", or has no value.
DCP02RS is the coded term of the period 2 event, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
