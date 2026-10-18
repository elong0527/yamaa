Following CDISC SDTM standards, use the provided DM and ODM datasets to create
an SE dataset with one record per subject per element the subject
entered.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SESEQ, ETCD, ELEMENT, TAETORD, EPOCH, SESTDTC,
SEENDTC, SESTDY, SEENDY, SEUPDES

DOMAIN is "SE". SESTDY is the study day of SESTDTC counted from the
subject's reference start date in DM, and SEENDY is the study day of
SEENDTC counted the same way. SEUPDES has no value on every record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/se.csv.
