Following CDISC SDTM standards, use the provided EC_RAW dataset to
create an EX dataset with one record per constant-dose interval.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXDOSFRQ, EXSTDTC,
EXENDTC, EXADJ

The subject's administrations of one treatment at one dose, unit, and
frequency form one interval. EXSTDTC is the first administration date
at the interval dose level, and EXENDTC is the last. EXADJ is the
adjustment reason recorded on any administration in the interval, such
as DOSE REDUCED or DOSE INTERRUPTED, with no value when none was
recorded; when the recorded reasons differ, use the alphabetically
first. A dose interruption is its own interval with a zero dose and the
recorded reason. A dose level that recurs later is not split: its one
record spans from the level's first to its last administration. EXSEQ
numbers the subject's intervals in the order they started, by start
date then treatment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
