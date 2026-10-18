Following CDISC SDTM standards, use the provided DIARY dataset to create an FA
dataset with one record per test for each solicited reaction on each diary
day.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FASCAT,
FAORRES, FAORRESU, FASTRESC, FASTRESN, FASTRESU, FASTAT, FATPT, FADTC

DOMAIN is "FA". STUDYID and USUBJID are the study and subject identifiers
as given in the diary. Every reaction on every diary day gives an "OCCUR"
record and, on a completed day, a "SEV" record; redness or swelling that
occurred on a completed day also gives an "LDIAM" record. FATEST is
"Occurrence Indicator" for "OCCUR", "Severity/Intensity" for "SEV", and
"Longest Diameter" for "LDIAM". FAOBJ is the solicited reaction. FACAT is
"REACTOGENICITY" on every record. FASCAT is "ADMINISTRATION SITE" for a
local reaction and "SYSTEMIC" for a systemic reaction. FAORRES on an
"OCCUR" record is the recorded occurrence, "Y" or "N", on a completed day,
and has no value on a missed diary day. FAORRES on a "SEV" record is the
recorded severity, or "NONE" when the reaction did not occur. FAORRES on
an "LDIAM" record is the measured diameter as recorded. FASTRESC is the
same value as FAORRES. FAORRESU and FASTRESU are the diameter unit on an
"LDIAM" record and have no value on any other record. FASTRESN is the
measured diameter as a number on an "LDIAM" record and has no value on
any other record. FASTAT is "NOT DONE" for a missed diary day and has no
value otherwise. FATPT is the diary day label, such as "END DAY 1". FADTC
is the diary collection date. FASEQ numbers each subject's records from 1
by diary day, then reaction name, then test order ("OCCUR", "SEV",
"LDIAM").

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
