Following CDISC ADaM standards, use the provided EX dataset to create an
ADEX dataset with one record per infusion.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXSEQ, EXSTDTC, EXENDTC, EXDUR, PARAMCD, PARAM, AVAL, AVALU,
CALCDUR

Every EX record keeps its record. PARAMCD is INFDUR, PARAM is Infusion
Duration (min) and AVALU is min on every record. CALCDUR is the elapsed
time in minutes from EXSTDTC to EXENDTC, with seconds kept as a fraction
of a minute and nothing added or rounded; a time collected without
seconds counts as second zero. CALCDUR has no value when the start or the
end was collected as a date without a time of day: no time is assumed.
AVAL is the collected duration EXDUR in minutes when EXDUR is a readable
ISO 8601 duration of days, hours, minutes and seconds, such as PT1H30M
for 90 minutes; otherwise AVAL is CALCDUR. Text that is not such a
duration, for example one without its leading P, counts as not collected.
A readable collected duration always wins, even when it disagrees with
the start and end times. AVAL has no value when neither gives a value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
