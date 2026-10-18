Following CDISC ADaM standards, use the provided EX dataset to create an ADEX
dataset with one record per infusion.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXSEQ, EXSTDTC, EXENDTC, EXDUR, PARAMCD, PARAM, AVAL, AVALU,
CALCDUR

PARAMCD is "INFDUR" and PARAM is "Infusion Duration (min)" on every record.
AVALU is "min" on every record. CALCDUR is the elapsed time in minutes from
EXSTDTC to EXENDTC, with seconds kept as a fraction of a minute and nothing
added or rounded. CALCDUR has no value when EXSTDTC or EXENDTC was collected
as a date without a time of day; no time is assumed. AVAL is the collected
duration in minutes when EXDUR holds a readable fixed-length ISO 8601
duration; otherwise AVAL is CALCDUR. Text that is not such a duration, for
example one without its leading P, counts as not collected. A readable
collected duration always wins, even when it disagrees with the start and
end times. AVAL has no value when neither gives a value, and every infusion
keeps its record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
