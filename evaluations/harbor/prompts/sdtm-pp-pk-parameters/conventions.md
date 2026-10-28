Following CDISC SDTM standards, use the provided NCA_TRANSFER dataset to create
a PP dataset with one record per subject, analyte, reference dose and supplied
PK parameter.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PPSEQ, PPTESTCD, PPTEST, PPCAT, PPORRES, PPORRESU,
PPSTRESC, PPSTRESN, PPSTRESU, PPSPEC, PPRFTDTC

DOMAIN is "PP".
For software parameter "Cmax", PPTESTCD is "CMAX" and PPTEST is "Max Conc"; for
"Tmax", they are "TMAX" and "Time of CMAX Observation"; for "AUClast", they are
"AUCLST" and "AUC to Last Nonzero Conc".
PPORRES and PPSTRESC include every reported decimal place.
PPSTRESN is reported without rounding.
PPSPEC is "PLASMA".

Read the source datasets from /app/input and save the completed dataset as
/app/output/pp.csv.
