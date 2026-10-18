Following CDISC SDTM standards, use the provided AE, DM_RAW, DS, and EX
datasets to create a DM dataset with one record per enrolled subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, RFICDTC, RFXSTDTC, RFXENDTC, RFSTDTC, RFPENDTC,
RFENDTC

DOMAIN is "DM". Exposure, disposition, and adverse event records count
only for the subject with the same STUDYID and USUBJID.

RFXSTDTC is the earliest exposure start date, and has no value when no
exposure record of the subject carries a start date. RFXENDTC is the
latest exposure end date, and has no value when no exposure record of
the subject carries an end date. RFSTDTC is the first exposure date: the
same date as RFXSTDTC, with no value whenever RFXSTDTC has no value.

RFPENDTC is the last date the subject is known to have participated:
the latest of the last exposure end date, the latest disposition event
date, and the latest adverse event end date. A disposition event is a
disposition record whose category is "DISPOSITION EVENT", and its date
is the record's start date. A date that has no value is left out, and
RFPENDTC has no value only when all three dates have no value.

RFENDTC is the end of the subject's reference period: it is the same
date as RFPENDTC, and it has no value whenever RFSTDTC has no value, so
a subject who never entered the reference period has no reference end
date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
