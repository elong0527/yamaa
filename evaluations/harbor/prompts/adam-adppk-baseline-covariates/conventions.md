Following CDISC ADaM standards, use the provided ADSL, EX, LB, PC, and VS
datasets to create an ADPPK dataset with one record per subject per
administration or pharmacokinetic sample.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADTM, EVID, AGE, SEX, SEXN, RACE, RACEN, WTBL, HTBL,
BMIBL, BSABL, CREATBL, ALTBL, TBILBL, CRCLBL, EGFRBL

ADTM is in the form YYYY-MM-DDThh:mm:ss with the seconds shown. EVID is 1
for a dose record and 0 for a concentration record. SEXN is 1 for "M" and 2
for "F". RACEN is 2 for "ASIAN", 3 for "BLACK OR AFRICAN AMERICAN", 5 for
"WHITE", or 6. BSABL is the Mosteller body surface area, CRCLBL the
Cockcroft-Gault creatinine clearance, and EGFRBL the CKD-EPI 2021 estimated
glomerular filtration rate. BMIBL, BSABL, CRCLBL and EGFRBL are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
