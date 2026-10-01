Following CDISC SDTM standards, use the provided DIARY dataset to
create an FA dataset with one record per solicited reaction test on
each diary day.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FASCAT,
FAORRES, FAORRESU, FASTRESC, FASTRESN, FASTRESU, FASTAT, FATPT, FADTC

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
