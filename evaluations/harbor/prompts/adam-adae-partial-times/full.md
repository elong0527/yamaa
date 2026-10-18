Following CDISC ADaM standards, use the provided AE dataset to create an ADAE
dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, ASTDTM, ASTTMF, AEENDTC, AENDTM,
AENTMF

ASTDTM is the analysis start date/time, completed from AESTDTC. A start
collected to the second is used as it stands. Each part of the start time
that was not collected takes its earliest value, so a start collected to
the hour is at minute 00 and second 00, and a date alone starts at
00:00:00. ASTDTM has no value when AESTDTC was collected without its day
or not collected at all. ASTTMF is "H" when the start was collected as a
date alone, "M" when it was collected to the hour, and "S" when it was
collected to the minute. ASTTMF has no value when the start was collected
to the second or when ASTDTM has no value. AENDTM is the analysis end
date/time, completed from AEENDTC in the same way but with the latest
value of each part of the time that was not collected, so an end
collected to the hour is at minute 59 and second 59, and a date alone
ends at 23:59:59. An end collected to the second is used as it stands.
AENDTM has no value when AEENDTC was collected without its day or not
collected at all. AENTMF is "H" when the end was collected as a date
alone, "M" when it was collected to the hour, and "S" when it was
collected to the minute. AENTMF has no value when the end was collected
to the second or when AENDTM has no value. Each flag records what was
collected, not whether the completed time looks different: a start
collected at hour 00 is flagged "M" although it completes to the same
time as a date alone.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
