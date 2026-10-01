Following CDISC SDTM standards, use the provided LOG dataset to create
an EC dataset with one record per dosing-log day.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, ECSEQ, ECTRT, ECMOOD, ECOCCUR, ECREASND,
ECDOSE, ECDOSU, ECDOSFRM, ECDOSFRQ, ECROUTE, ECSTDTC, ECENDTC, ECADJ

ECTRT is Study Drug and ECMOOD is PERFORMED on every record. ECOCCUR is Y or
N. ECDOSFRM is TABLET, ECDOSFRQ is QD, and ECROUTE is ORAL.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ec.csv.
