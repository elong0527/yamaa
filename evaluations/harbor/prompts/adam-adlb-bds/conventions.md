Following CDISC ADaM standards, use the provided ADSL and LB datasets to create
an ADLB dataset with one record per subject per parameter per collection
date.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRTSDT, TRT01A, AVAL, AVALU, ABLFL,
BASE, CHG, PCHG, ASEQ

PARAMCD is "ALT" and PARAM is "Alanine Aminotransferase", PARAMCD is
"ALTSI" and PARAM is "Alanine Aminotransferase (SI)", or PARAMCD is "AST"
and PARAM is "Aspartate Aminotransferase". ABLFL is "Y" or has no value.
AVAL, CHG, and PCHG are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
