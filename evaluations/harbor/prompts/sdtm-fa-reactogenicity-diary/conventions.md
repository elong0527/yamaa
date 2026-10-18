Following CDISC SDTM standards, use the provided DIARY dataset to create an FA
dataset with one record per test for each solicited reaction on each diary
day.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FASCAT,
FAORRES, FAORRESU, FASTRESC, FASTRESN, FASTRESU, FASTAT, FATPT, FADTC

DOMAIN is "FA". FATEST is "Occurrence Indicator" for "OCCUR",
"Severity/Intensity" for "SEV", and "Longest Diameter" for "LDIAM". FACAT
is "REACTOGENICITY" on every record. FASCAT is "ADMINISTRATION SITE" or
"SYSTEMIC". FAORRES on an "OCCUR" record is "Y" or "N", or has no value.
FAORRES on a "SEV" record is the recorded severity, or "NONE". FASTAT is
"NOT DONE" or has no value. FATPT is the diary day label, such as "END
DAY 1".

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
