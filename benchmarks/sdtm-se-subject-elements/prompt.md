Following CDISC SDTM standards, use the provided DM and ODM datasets to
create an SE dataset with one record per subject per element the subject
entered.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SESEQ, ETCD, ELEMENT, TAETORD, EPOCH, SESTDTC,
SEENDTC, SESTDY, SEENDY, SEUPDES

SESEQ numbers the subject's elements in planned order, so it equals
TAETORD. ETCD is the element code from the subject's study event, ELEMENT
its description, TAETORD its planned order, and EPOCH its epoch.

SESTDTC is the date the subject actually started the element: informed
consent for screening, the first dosing date for treatment, and the last
dosing date for follow-up (and for any other or missing epoch). SEENDTC is
the date the subject actually ended the element: the first dosing date for
screening, the last dosing date for treatment, and the end of study
participation for follow-up (and for any other or missing epoch). SESTDY
and SEENDY are the study days of the element start and end, counting the
reference start date as day 1 with no day zero; each has no value when
either date is missing. SEUPDES always has no value: the records are built
only from planned elements.

Elements are recorded back to back: an element ends on the day the next one
starts, so a subject who stopped dosing early has a shorter treatment
element and an earlier follow-up start.

Read the source datasets from /app/input and save the completed dataset as
/app/output/se.csv.
