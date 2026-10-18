Following CDISC ADaM standards, use the provided AE dataset to create an
ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, ASTDT, ASTTM, ASTTMF

ASTDTM is the analysis start date and time from AESTDTC, written as
YYYY-MM-DDThh:mm:ss. A time collected to the second is used as collected.
Each part of the time that was not collected takes its earliest value: a
time collected to the minute gets second 00, a time collected to the hour
gets minute and second 00, and a date alone starts at 00:00:00. A start
collected without its day gives no ASTDTM. ASTDT is the date part of
ASTDTM. ASTTM is the time of day of ASTDTM as a whole number of seconds
since midnight, so 08:30:15 is 30615 and midnight is 0. ASTTMF names the
first part of the time that was not collected and had to be imputed: H
when the start was collected without a time, M when it stops at the hour,
and S when it stops at the minute. ASTDT, ASTTM and ASTTMF have no value
when there is no ASTDTM, and ASTTMF also has no value when the time was
collected to the second.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
