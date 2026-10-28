Following CDISC SDTM standards, use the provided NCA_TRANSFER dataset to create
a PP dataset with one record per supplied pharmacokinetic parameter estimate.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PPSEQ, PPTESTCD, PPTEST, PPCAT, PPORRES, PPORRESU,
PPSTRESC, PPSTRESN, PPSTRESU, PPSPEC, PPRFTDTC

DOMAIN is "PP". For Parameter "Cmax", "Tmax" and "AUClast", PPTESTCD is
"CMAX", "TMAX" and "AUCLST", respectively. PPTEST is "Max Conc" for "CMAX",
"Time of CMAX Observation" for "TMAX", and "AUC to Last Nonzero Conc" for
"AUCLST". PPORRES and PPSTRESC include all reported decimal places and
trailing zeros. PPSTRESN is not rounded. PPSPEC is "PLASMA".
PPRFTDTC is in "YYYY-MM-DDThh:mm:ss" form.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pp.csv.
