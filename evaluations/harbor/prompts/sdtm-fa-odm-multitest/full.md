Following CDISC SDTM standards, use the provided ODM dataset to create an FA
dataset with one record per collected reaction finding.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES,
FAORRESU, FASTRESC, FASTAT, FATPT, FADTC

DOMAIN is "FA". STUDYID is the ODM study OID and USUBJID is the ODM
subject key, each as given. One record is written for each occurrence,
severity, or longest diameter result item collected for a reaction. A
reaction with no severity or diameter item has no such record, while a
result item that is present with a blank result still gives its record.
FATESTCD is "OCCUR" for the occurrence result, "SEV" for the severity
result, and "LDIAM" for the longest diameter result, and FATEST is
"Occurrence Indicator", "Severity/Intensity", and "Longest Diameter" for
the same results. FAOBJ is the reaction name exactly as reported in the
reaction name item of the same diary occurrence. FACAT is
"REACTOGENICITY" for every record. FAORRES is the result as collected,
and FASTRESC holds the same result. FAORRESU is "mm" on a longest
diameter record and has no value on an occurrence or severity record.
FASTAT is "NOT DONE" when a result item is present but its result is
blank, and has no value when a result is reported. FATPT is "END DAY 1"
for diary day DAY1 and "END DAY 2" for diary day DAY2. FADTC is the
collection date collected for the same diary occurrence. FASEQ numbers a
subject's records from 1 by diary day, then by the diary day's repeat,
then by reaction repeat, then by test order, occurrence first, then
severity, then longest diameter.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
