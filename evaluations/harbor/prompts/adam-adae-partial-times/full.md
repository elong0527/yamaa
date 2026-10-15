Following CDISC ADaM standards, use the provided AE dataset to create an
ADAE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, ASTDTM, ASTTMF, AEENDTC, AENDTM,
AENTMF

ASTDTM is the analysis start date and time from AESTDTC, written as
YYYY-MM-DDThh:mm:ss. A time collected to the second is used as
collected. Each part of the time that was not collected takes its
earliest value: a time collected to the minute gets second 00, a time
collected to the hour gets minute and second 00, and a date alone starts
at 00:00:00. A start collected without its day, or not collected at all,
gives no ASTDTM. AENDTM is the analysis end date and time from AEENDTC,
completed the same way but with the latest value of each missing part:
second 59, minute 59, and a date alone ends at 23:59:59. ASTTMF and
AENTMF name the coarsest part of the time that was supplied for ASTDTM
and AENDTM: H when the hour was supplied (a date alone was collected), M
when the minutes were supplied (a time collected to the hour), and S when
only the seconds were supplied (a time collected to the minute). A flag
has no value when the time was collected to the second and when there is
no analysis date and time. A flag records what was collected: a start
collected as 08:30 is flagged S although it completes to the same moment
as one collected as 08:30:00.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adae.csv.
