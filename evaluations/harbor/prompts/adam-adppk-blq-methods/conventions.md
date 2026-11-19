Following CDISC ADaM standards, use the provided EX and PC datasets to create
an ADPPK dataset with one record per subject per administration (a dose
record) or sample that was done (a concentration record).

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADTM, EVID, AMT, ALLOQ, CENS, DV, MDV, DVM6, MDVM6

ADTM is written as YYYY-MM-DDThh:mm:ss with its seconds. EVID is 1 on a dose
record and 0 on a concentration record. CENS is 1 for a result below the
limit of quantification, and 0 for a quantified result and on a dose record.
MDV is 1 when DV has no value and 0 otherwise, and MDVM6 is 1 when DVM6 has
no value and 0 otherwise. DVM6 is not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
