Following CDISC SDTM standards, use the provided KIT_LIST and ODM datasets to
create an EX dataset with one record per administration.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXSTDTC, EXENDTC

DOMAIN is "EX". EXDOSE is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
