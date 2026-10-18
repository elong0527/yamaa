Following CDISC SDTM standards, use the provided DM and ODM datasets to create
an SV dataset with one record per subject per visit.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SVSEQ, VISIT, VISITNUM, VISITDY, SVSTDTC, SVENDTC,
SVSTDY, SVENDY, TAETORD, EPOCH, SVUPDES

DOMAIN is "SV" on every record. STUDYID is the study OID and USUBJID the
subject key, each as given in ODM. Each study event occurrence for a subject
gives one record, and a repeated unscheduled event gives a record of its own.
VISIT is the visit name carried on the study event. VISITNUM is the planned
visit number carried on the study event, and distinct numbers starting at
99.1 identify a subject's unscheduled visits. VISITDY is the planned study
day carried on the study event, and it has no value for an unscheduled visit,
which carries no plan. SVSTDTC is the earliest of the form dates the visit's
VS, LB and EX forms carry, and SVENDTC is the latest of them, so a visit
whose forms were collected on different days spans them. SVSTDY is the study
day of the visit start and SVENDY the study day of the visit end, each
counted from the subject's RFSTDTC in DM. TAETORD is the planned order of the
element within the arm carried on the study event, and it has no value for
an unscheduled visit. EPOCH is the epoch carried on the study event. SVUPDES
is the description the site entered for the unplanned visit, carried on the
unscheduled study event, and it has no value for a planned visit. SVSEQ
numbers each subject's visits from 1 in order of SVSTDTC, so an unscheduled
visit takes its place between the planned visits around it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/sv.csv.
