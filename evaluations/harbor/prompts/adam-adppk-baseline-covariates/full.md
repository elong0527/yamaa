Following CDISC ADaM standards, use the provided ADSL, EX, LB, PC, and VS
datasets to create an ADPPK dataset with one record per subject per
administration or pharmacokinetic sample.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, ASEQ, ADTM, EVID, AGE, SEX, SEXN, RACE, RACEN, WTBL, HTBL,
BMIBL, BSABL, CREATBL, ALTBL, TBILBL, CRCLBL, EGFRBL

Each EX record gives a dose record and each PC record a concentration
record. Every record holds its subject's baseline covariates.

- ADTM is the date/time of the administration (EXSTDTC) or of the sample
  (PCDTC), in the form YYYY-MM-DDThh:mm:ss with the seconds shown.
- ASEQ numbers the subject's records from 1 in the order of ADTM.
- EVID is 1 for a dose record and 0 for a concentration record.
- AGE, SEX and RACE are as in ADSL. SEXN is 1 for "M" and 2 for "F". RACEN
  is 2 for "ASIAN", 3 for "BLACK OR AFRICAN AMERICAN", 5 for "WHITE" and 6
  for any other value.
- WTBL and HTBL are the baseline results (VSSTRESN) of the VS tests
  "WEIGHT" and "HEIGHT", and CREATBL, ALTBL and TBILBL the baseline results
  (LBSTRESN) of the LB tests "CREAT", "ALT" and "BILI". The baseline record
  of a test is the one flagged "Y" in VSBLFL or LBBLFL, and each of these
  has no value when the subject has none. Results that are not flagged as
  baseline, and tests other than those named, are not used.
- BMIBL is WTBL divided by the square of HTBL in metres (HTBL / 100), and
  BSABL is the Mosteller body surface area, the square root of
  HTBL x WTBL / 3600.
- CRCLBL is the Cockcroft-Gault creatinine clearance in mL/min:
  (140 - AGE) x WTBL x 1.23 / CREATBL for a male subject, with 1.04 in
  place of 1.23 for a female subject.
- EGFRBL is the CKD-EPI 2021 estimated glomerular filtration rate in
  mL/min/1.73 m2. With r = CREATBL / 88.42 / 0.9 for a male subject and
  CREATBL / 88.42 / 0.7 for a female subject, it is 142 x r^a x 0.9938^AGE,
  times 1.012 for a female subject, where the exponent a is -1.2 when r is
  above 1 and otherwise -0.302 for a male and -0.241 for a female subject.
- BMIBL, BSABL, CRCLBL and EGFRBL are not rounded, and each has no value
  when a value it is computed from has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adppk.csv.
