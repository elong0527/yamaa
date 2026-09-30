Following CDISC ADaM standards, use the provided ADSL_RAW, ADAE, and
ADVS datasets to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTEDT, LSTCNTDC, LSTCNTDT, LSTALVDT

LSTCNTDT is the contact text completed to a day: a year and month take
the first day of that month, a year alone takes the first day of
January, and missing or unusable text leaves the date missing.

LSTALVDT is the latest of the treatment end date, the completed
contact date, the subject's latest adverse event end date, and the
subject's latest vital signs date. A completed date competes on the
day it names, so a fully collected date within that period wins. When
every source is missing the date stays missing; otherwise the latest
available date is kept.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
