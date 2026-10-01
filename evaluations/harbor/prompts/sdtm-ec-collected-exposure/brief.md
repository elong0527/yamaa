Following CDISC SDTM standards, use the provided LOG dataset to create
an EC dataset with one record per dosing-log day.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ECSEQ, ECTRT, ECMOOD, ECOCCUR, ECREASND,
ECDOSE, ECDOSU, ECDOSFRM, ECDOSFRQ, ECROUTE, ECSTDTC, ECENDTC, ECADJ

Read the source datasets from /app/input and save the completed dataset as
/app/output/ec.csv.
