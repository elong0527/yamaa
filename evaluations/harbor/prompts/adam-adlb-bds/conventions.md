Following CDISC ADaM standards, use the provided LB and ADSL datasets
to create an ADLB dataset with one record per subject per parameter
per collection date.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRTSDT, TRT01A, AVAL, AVALU,
ABLFL, BASE, CHG, PCHG, ASEQ

Use these parameters:
- PARAMCD "ALT" with PARAM "Alanine Aminotransferase" and "AST" with
  PARAM "Aspartate Aminotransferase";
- PARAMCD "ALTSI" with PARAM "Alanine Aminotransferase (SI)", holding the
  ALT result times 0.0167 with AVALU "ukat/L".

ABLFL is Y or has no value.

AVAL, BASE, CHG, and PCHG are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
