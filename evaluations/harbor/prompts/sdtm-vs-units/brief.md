Following CDISC SDTM standards, use the provided VS_RAW dataset to create
a VS dataset with one record per collected vital-signs measurement.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSSTAT

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
