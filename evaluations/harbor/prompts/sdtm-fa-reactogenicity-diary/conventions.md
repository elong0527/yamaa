Following CDISC SDTM standards, use the provided DIARY dataset to
create an FA dataset with one record per solicited reaction test on
each diary day.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FASCAT,
FAORRES, FAORRESU, FASTRESC, FASTRESN, FASTRESU, FASTAT, FATPT, FADTC

FATESTCD is OCCUR, SEV, or LDIAM, with FATEST Occurrence Indicator,
Severity/Intensity, or Longest Diameter. FACAT is always REACTOGENICITY.
FASCAT is ADMINISTRATION SITE for local reactions and SYSTEMIC for systemic
reactions. FAORRES is the recorded Y or N for OCCUR, the recorded severity
or NONE for SEV, and the measured diameter as recorded for LDIAM (a whole
number has no decimal point). FASTAT is NOT DONE or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
