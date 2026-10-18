Following CDISC ADaM standards, use the provided ADAE, ADSL, and ADVS datasets
to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRTEDT, LSTCNTDC, LSTCNTDT, LSTALVDT

LSTCNTDT is the date in LSTCNTDC completed to a day. A year and month
take the first day of that month, and a year alone takes the first day
of January. LSTCNTDT has no value when LSTCNTDC has no value or holds
text that cannot be read as a date. LSTALVDT is the latest of TRTEDT,
LSTCNTDT, the subject's latest AENDT in ADAE, and the subject's latest
ADATE in ADVS. A completed date competes on the day it names, and
completing a contact to the start of its month or year claims no
contact beyond what was collected. LSTALVDT has no value when every
one of these dates has no value; otherwise it is the latest of the
dates that have a value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
