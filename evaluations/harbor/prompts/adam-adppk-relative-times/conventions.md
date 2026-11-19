Following CDISC ADaM standards, use the provided EX and PC datasets to create
an ADPPK dataset with one record per subject per dose or PK concentration
record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADTM, EVID, AMT, FANLDTM, AFRLT, APRLT, DOSEA

Date/times are clock readings taken as written, with no time zone. EVID is
1 on a dose record and 0 on a concentration record. AFRLT and APRLT are not
rounded. ADTM and FANLDTM are written as YYYY-MM-DDThh:mm:ss, with the
seconds always shown.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
