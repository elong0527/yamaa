Following CDISC ADaM standards, use the provided ADSL, EX, LB, PC, and VS
datasets to create an ADPPK dataset with one record per administration in
EX and per sample in PC.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADTM, EVID, WT, CREAT, CRCL

- EVID is 1 for a dose record and 0 for a concentration record.
- ADTM is written as YYYY-MM-DDThh:mm:ss, with the seconds always shown.
- CRCL is the Cockcroft-Gault creatinine clearance, not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
