Following CDISC SDTM standards, use the provided DM and ODM datasets to
create an SV dataset with one record per subject per visit attended.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SVSEQ, VISIT, VISITNUM, VISITDY, SVSTDTC,
SVENDTC, SVSTDY, SVENDY, TAETORD, EPOCH, SVUPDES

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

SVSEQ numbers each subject's visits in date order. VISIT is the visit name
carried on the study event. VISITNUM is the planned visit number;
distinct numbers starting at 99.1 identify a subject's unscheduled visits.
VISITDY is the planned study day of the visit, and has no value for
unscheduled visits. TAETORD is the planned order of the element within the
arm, and has no value for unscheduled visits. EPOCH is the epoch carried
on the study event.

SVSTDTC is the start date of the visit, the earliest form date among the
visit's VS, LB, and EX forms; SVENDTC is the end date, the latest form
date. A visit whose forms were collected on different days spans them.
SVSTDY and SVENDY are the study days of the visit start and end, counted
from the subject's reference start date with that date as day 1 and no day
zero. SVUPDES is the description the site entered for the unplanned visit,
and has no value for planned visits.

Visits are numbered by their start date, so an unscheduled visit takes its
place between the planned visits around it. Repeated unscheduled visits
retain separate identities.

Read the source datasets from /app/input and save the completed dataset as
/app/output/sv.csv.
