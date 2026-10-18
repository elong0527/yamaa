Following CDISC ADaM standards, use the provided AE and EX datasets to create
an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, ASTDTM, DOSE24FL, DOSENUM

ASTDT is the date of the collected onset, and has no value when no onset
was collected. ASTDTM is the collected onset date and time, and has no
value when the onset was collected as a date alone; no time is imputed.
DOSE24FL is "Y" when the onset is at or after the start of one of the
subject's doses and before 24 hours have passed since that start: an
onset exactly at a dose start is inside, and an onset exactly 24 hours
after a dose start is outside. DOSE24FL has no value for an onset before
the subject's first dose, for an onset collected without a time, and when
no onset was collected. DOSENUM is the number of the dose whose 24 hours
hold the onset, counting the subject's doses 1, 2, 3 in order of their
start, and has no value when DOSE24FL has no value. When two doses start
less than 24 hours apart and the onset falls within 24 hours of both,
DOSENUM is the number of the later dose. Times compare to the second, and
a time collected without seconds counts as second zero.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
