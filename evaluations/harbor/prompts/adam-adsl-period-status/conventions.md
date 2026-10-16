Following CDISC ADaM standards, use the provided DM, EX and DS
datasets to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EOP01STT, DCP01RS, EOP02STT, DCP02RS

Period 1 is the TREATMENT 1 epoch and period 2 is the TREATMENT 2
epoch.

EOP01STT and EOP02STT are COMPLETED, DISCONTINUED, or ONGOING, or
have no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
