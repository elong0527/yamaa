Following CDISC ADaM standards, use the provided DM and DS datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EOSDT, EOSREAS, EOSSTT, DCSREAS, EOTDT, EOTSTT, DCTREAS,
DCTREASP

EOSSTT is "COMPLETED", "DISCONTINUED", or "ONGOING", or has no value for a
screen failure. EOSDT and EOSREAS also have no value for a screen failure.
EOTSTT is "COMPLETED", "DISCONTINUED", or "ONGOING", or has no value for a
subject who never received study treatment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
