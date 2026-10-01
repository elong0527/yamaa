Following CDISC SDTM standards, use the provided ODM dataset to create
an FA dataset with one record per collected reaction finding.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES,
FAORRESU, FASTRESC, FASTAT, FATPT, FADTC

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

FATESTCD and FATEST are OCCUR and Occurrence Indicator, SEV and
Severity/Intensity, or LDIAM and Longest Diameter. FACAT is REACTOGENICITY for
every diary finding. FAORRESU is mm for diameter only. FASTAT is NOT DONE or
has no value. FATPT is END DAY 1 for a DAY1 occurrence and END DAY 2 for a DAY2
occurrence. FASEQ numbers the subject's findings by diary day, reaction repeat,
and test order.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
