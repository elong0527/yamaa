Following CDISC ADaM standards, use the provided AE and EX datasets to
create an ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AEDECOD, ASTDT, ASTDTM, DOSE24FL, DOSENUM

ASTDT is the onset date, and has no value when no onset was collected.
ASTDTM is the onset date and time, filled only when a time of day was
collected: no time is imputed, so an onset recorded as a date alone has
no value here. Times compare to the second, and a time collected
without seconds counts as second zero.

DOSE24FL is Y when the onset is at or after a dose start and before
24 hours have passed since that start: an onset exactly at the start
is inside, and one exactly 24 hours later is outside. An onset before
the first dose, an onset without a time, and a missing onset have no
value. DOSENUM is the number of the dose whose 24 hours hold the
onset, counting the subject's doses 1, 2, 3 in order of start, and has
no value when DOSE24FL has no value. When two doses start less than
24 hours apart and the onset falls within both, the later dose is
counted.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
