Following CDISC SDTM standards, use the provided ODM dataset to create
an FA dataset with one record per collected reaction finding.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FAOBJ, FACAT, FAORRES,
FAORRESU, FASTRESC, FASTAT, FATPT, FADTC

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

FATESTCD and FATEST are OCCUR and Occurrence Indicator, SEV and
Severity/Intensity, or LDIAM and Longest Diameter, following the
collected result item. FAOBJ is the reaction name exactly as reported
in the sibling item of the same diary occurrence. FACAT is
REACTOGENICITY for every diary finding. FAORRES and FASTRESC keep the
collected result; FAORRESU is mm for diameter only. FASTAT is NOT DONE
for a result item that is present but blank, and has no value for
reported results. FATPT is END DAY 1 for a DAY1 occurrence and END DAY
2 for a DAY2 occurrence, and FADTC is that day's collection date from
the sibling item. A reaction without a severity or diameter item gives
no such test, while a present item with a blank result still gives its
record. A form repeat number reused on another diary day belongs to
that day. FASEQ numbers the subject's findings by diary day, reaction
repeat, and test order.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
