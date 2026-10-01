Following CDISC SDTM standards, use the provided DM and ODM datasets to
create an SE dataset with one record per subject per element the subject
entered.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SESEQ, ETCD, ELEMENT, TAETORD, EPOCH, SESTDTC,
SEENDTC, SESTDY, SEENDY, SEUPDES

SEUPDES always has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/se.csv.
