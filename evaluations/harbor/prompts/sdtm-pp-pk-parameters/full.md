Following CDISC SDTM standards, use the provided NCA_TRANSFER dataset to create
a PP dataset with one record per supplied analysis parameter estimate.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PPSEQ, PPTESTCD, PPTEST, PPCAT, PPORRES, PPORRESU,
PPSTRESC, PPSTRESN, PPSTRESU, PPSPEC, PPRFTDTC

DOMAIN is "PP" and PPSPEC is "PLASMA".
STUDYID is the supplied Protocol identifier and USUBJID is the supplied Subject
identifier, without alteration.
PPSEQ starts at 1 within each subject and study, increasing by reference dose
date and time, then analyte, then standard parameter code, all ascending.
For software parameters "Cmax", "Tmax" and "AUClast", PPTESTCD is "CMAX",
"TMAX" and "AUCLST", respectively.
PPTEST is "Max Conc" for "CMAX", "Time of CMAX Observation" for "TMAX" and "AUC
to Last Nonzero Conc" for "AUCLST".
PPCAT identifies the supplied analyte.
PPORRES and PPSTRESC are the supplied estimate as text, preserving all reported
decimal places, including trailing zeros.
PPSTRESN is the numeric value of that estimate, without rounding or
recalculation from concentration profiles.
PPORRESU and PPSTRESU are the supplied analysis unit, without conversion or
assignment of a unit from the parameter code.
PPRFTDTC identifies the supplied reference dose date and time.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pp.csv.
