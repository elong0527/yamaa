Following CDISC SDTM standards, use the provided EX_RAW dataset to create an EX
dataset with one record per administration, that is, per subject per EXSEQ.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXSTDTC, EXDUR

DOMAIN is "EX". EXDUR is ISO 8601 duration text: "P", then the days, then "T"
and the hours and minutes, each part that is zero left out. So 90 minutes is
"PT1H30M", and a day of 24 hours is the largest part, so a 46-hour continuous
infusion is "P1DT22H", never "PT46H". Zero minutes is "PT0S", or EXDUR has no
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
