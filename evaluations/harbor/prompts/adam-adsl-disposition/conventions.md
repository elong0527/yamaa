Following CDISC ADaM standards, use the provided DM and DS datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EOSDT, EOSDECOD, EOSREAS, EOSSTT, DCSREAS,
EOTDT, EOTSTT, DCTREAS, DCSREASP

EOSSTT is COMPLETED, DISCONTINUED, or ONGOING. EOTSTT follows the
same rule over the subject's treatment disposition records (epoch
TREATMENT).

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
