Following CDISC SDTM standards, use the provided DIARY dataset to
create an FA dataset with one record per solicited reaction test on
each diary day.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FASCAT,
FAORRES, FAORRESU, FASTRESC, FASTRESN, FASTRESU, FASTAT, FATPT, FADTC

Every diary row gives an occurrence record; completed days also give a
severity record, and redness or swelling that occurred on a completed
day also gives a longest-diameter record. FATESTCD is OCCUR, SEV, or
LDIAM, with FATEST Occurrence Indicator, Severity/Intensity, or Longest
Diameter. FAOBJ keeps the solicited reaction. FACAT is always
REACTOGENICITY. FASCAT is ADMINISTRATION SITE for local reactions and
SYSTEMIC for systemic reactions. FAORRES is the recorded Y or N for
OCCUR, the recorded severity for SEV (NONE unless the reaction
occurred), and the measured diameter for LDIAM; it has no value on a
missed diary day. FAORRESU is the diameter unit, on LDIAM records only.
FASTRESC keeps FAORRES. FASTRESN is the measured diameter as a number,
on LDIAM records only. FASTRESU keeps the diameter unit, on LDIAM
records only. FASTAT is NOT DONE for a missed diary day, and has no
value otherwise. FATPT is the diary day label and FADTC is the diary
collection date. Temperature stays in Vital Signs and never becomes an
FA record here. FASEQ numbers the subject's records by diary day,
reaction name, and test order OCCUR, SEV, LDIAM.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
