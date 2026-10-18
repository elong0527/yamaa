Following CDISC ADaM standards, use the provided DM and DS datasets to create
an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EOSDT, EOSDECOD, EOSREAS, EOSSTT, DCSREAS, EOTDT, EOTSTT,
DCTREAS, DCSREASP

Only DS records with DSCAT "DISPOSITION EVENT" count; records in any other
category never count. The end-of-study summary reads the subject's
disposition records whose EPOCH is not "TREATMENT", and the end-of-treatment
summary reads those whose EPOCH is "TREATMENT", and the two summaries are
independent. A disposition record with no DSSTDTC never counts for either
summary. The last record of a summary is the subject's counted record with
the latest DSSTDTC, and when two counted records share that date, the one
with the higher DSSEQ is the last record.

EOSDT is the DSSTDTC of the last record of the study summary, and EOSDECOD
and EOSREAS are the DSDECOD and DSTERM on that same record. EOSDT, EOSDECOD,
and EOSREAS have no value when the subject has no dated study disposition
record. EOSSTT is "COMPLETED" when EOSDECOD is "COMPLETED", "DISCONTINUED"
when EOSDECOD is any other recorded term, and "ONGOING" when the subject has
no dated study disposition record. DCSREAS is EOSREAS when EOSSTT is
"DISCONTINUED"; it has no value otherwise.

EOTDT is the DSSTDTC of the last record of the treatment summary; it has no
value when the subject has no dated treatment disposition record. EOTSTT is
"COMPLETED" when the DSDECOD on the last record of the treatment summary is
"COMPLETED", "DISCONTINUED" when it is any other recorded term, and "ONGOING"
when the subject has no dated treatment disposition record. DCTREAS is the
DSTERM on the last record of the treatment summary when EOTSTT is
"DISCONTINUED"; it has no value otherwise. DCSREASP is that same DSTERM when
the DSDECOD on the last record of the treatment summary is "OTHER"; it has
no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
