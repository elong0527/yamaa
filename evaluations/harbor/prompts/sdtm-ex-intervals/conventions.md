Following CDISC SDTM standards, use the provided EC_RAW dataset to
create an EX dataset with one record per constant-dose interval.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXDOSFRQ, EXSTDTC,
EXENDTC, EXADJ

EXSEQ numbers the subject's intervals in the order they started, by start date
then treatment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
