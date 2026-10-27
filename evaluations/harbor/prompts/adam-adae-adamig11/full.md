Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, ASTDTM, ASTDT, ASTTM, ASTTMF

ASTDTM is the start date and time from AESTDTC: a start collected to the
second has its collected value, a start collected to the minute has seconds
00, a start collected to the hour has minutes and seconds 00, and a complete
date without a time has time 00:00:00. ASTDTM has no value when AESTDTC lacks
a complete calendar date.

ASTDT is the calendar date of ASTDTM and ASTTM is its time of day as whole
seconds since midnight; both have no value when ASTDTM has no value.

ASTTMF is "H" for a start collected as a complete date without a time, "M"
for a start collected to the hour, and "S" for a start collected to the
minute. ASTTMF has no value when the start was collected to the second or
ASTDTM has no value. An ASTTM of 0 with ASTTMF "H" is imputed midnight.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
