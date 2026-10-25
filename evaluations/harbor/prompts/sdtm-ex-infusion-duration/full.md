Following CDISC SDTM standards, use the provided EX_RAW dataset to create an EX
dataset with one record per administration, that is, per subject per EXSEQ.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXSTDTC, EXDUR

DOMAIN is "EX". Records are keyed by STUDYID, USUBJID and EXSEQ, where EXSEQ
is the sequence number EX_RAW gives each administration.

EXDUR is the duration collected in EXDURMIN, a whole number of minutes,
written as ISO 8601 duration text: "P", then the days, then "T" and the hours
and minutes, each part that is zero left out. So 120 minutes is "PT2H", 90
minutes is "PT1H30M" and 45 minutes is "PT45M". A day of 24 hours is the
largest part, so a 46-hour continuous infusion is "P1DT22H", never "PT46H".
A bolus injection recorded as zero minutes is "PT0S". EXDUR has no value when
EXDURMIN is empty; it is never computed from the start date.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
