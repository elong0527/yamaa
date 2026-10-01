Following CDISC SDTM standards, use the provided DM and ODM datasets to
create an SV dataset with one record per subject per visit attended.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SVSEQ, VISIT, VISITNUM, VISITDY, SVSTDTC,
SVENDTC, SVSTDY, SVENDY, TAETORD, EPOCH, SVUPDES

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

SVSEQ numbers each subject's visits in date order. VISITNUM is the planned
visit number; distinct numbers starting at 99.1 identify a subject's
unscheduled visits.

SVSTDY and SVENDY are study days, with no day zero.

Read the source datasets from /app/input and save the completed dataset as
/app/output/sv.csv.
