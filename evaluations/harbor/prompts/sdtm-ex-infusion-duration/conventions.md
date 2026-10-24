Following CDISC SDTM standards, use the provided EX_RAW dataset to create an EX
dataset with one record per administration.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXSTDTC, EXDUR

DOMAIN is "EX". EXDUR is ISO 8601 duration text: "P", then the days, then
"T" and the hours and minutes, each part that is zero left out, with a day
as the largest part, so 100 minutes is "PT1H40M" and 1500 minutes is
"P1DT1H"; a zero duration is "PT0S"; or EXDUR has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
