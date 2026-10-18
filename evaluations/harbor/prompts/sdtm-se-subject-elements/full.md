Following CDISC SDTM standards, use the provided DM and ODM datasets to create
an SE dataset with one record per subject per element the subject
entered.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SESEQ, ETCD, ELEMENT, TAETORD, EPOCH, SESTDTC,
SEENDTC, SESTDY, SEENDY, SEUPDES

DOMAIN is "SE". STUDYID is the study OID as given in ODM and USUBJID is
the subject key as given in ODM. ETCD is the code of the study event the
record is built from. ELEMENT is the element description the extract
gives for that event (the IT.TE.ELEMENT item). TAETORD is the planned
order of the element within the subject's arm (the IT.TA.TAETORD item).
EPOCH is the epoch the extract gives for that event (the IT.TA.EPOCH
item). SESEQ numbers the subject's elements in planned order and equals
TAETORD. The extract gives each subject a first dosing date and a last
dosing date, the same on every study event of the subject. SESTDTC is
the subject's informed consent date in DM when EPOCH is "SCREENING", the
subject's first dosing date when EPOCH is "TREATMENT", and the subject's
last dosing date for any other epoch or when EPOCH has no value. SEENDTC
is the subject's first dosing date when EPOCH is "SCREENING", the
subject's last dosing date when EPOCH is "TREATMENT", and the subject's
end of study participation date in DM for any other epoch or when EPOCH
has no value. SESTDY is the study day of SESTDTC counted from the
subject's reference start date in DM, and has no value when either date
has no value. SEENDY is the study day of SEENDTC counted the same way,
and has no value when either date has no value. SEUPDES has no value on
every record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/se.csv.
