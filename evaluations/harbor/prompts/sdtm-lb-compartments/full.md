Following CDISC SDTM standards, use the provided BX_RAW dataset to create an LB
dataset with one record per skin compartment per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBTEST, LBSPEC, LBLOC, LBORRES,
LBORRESU, LBSTRESN, LBSTAT

DOMAIN is "LB" on every record. LBTESTCD is "IL13" and LBTEST is
"Interleukin 13" on every record. LBSPEC is "SKIN" on every record.
Every subject has a non-lesional record, and a subject has a lesional
record as well when a cohort is recorded for the subject and the
cohort is not "NONAD". LBLOC is "LESIONAL" on the lesional record
and "NON-LESIONAL" on the non-lesional record. LBORRES is the
subject's lesional result on the lesional record and the subject's
non-lesional result on the non-lesional record, as reported; it has
no value when the compartment's sample was not analysed. LBORRESU
is the collected unit on every record, including a record whose
result has no value. LBSTRESN is the numeric form of the reported
result, and has no value when the reported result has no value.
LBSTAT is "NOT DONE" when the reported result has no value, and has
no value otherwise. LBSEQ numbers a subject's records from 1, the
lesional record before the non-lesional record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
