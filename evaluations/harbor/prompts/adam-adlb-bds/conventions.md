Following CDISC ADaM standards, use the provided ADSL and LB datasets to create
an ADLB dataset with one record per subject per parameter per collection
date.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARAMCD, PARAM, ADT, TRTSDT, TRT01A, AVAL, ABLFL, BASE, CHG,
PCHG, ASEQ

PARAMCD is "ALT" and PARAM is "Alanine Aminotransferase (U/L)", PARAMCD is
"ALTSI" and PARAM is "Alanine Aminotransferase (ukat/L)", or PARAMCD is "AST"
and PARAM is "Aspartate Aminotransferase (U/L)". ABLFL is "Y" or has no value.
AVAL, CHG, and PCHG are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adlb.csv.
