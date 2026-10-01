Following CDISC SDTM standards, use the provided EX_RAW dataset to
create an EX dataset with one record per administered combination
component.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXSTDTC, EXENDTC,
EXADJ

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
