Following CDISC SDTM standards, use the provided NCA_TRANSFER dataset to create
a PP dataset with one record per subject, analyte, reference dose and supplied
PK parameter.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PPSEQ, PPTESTCD, PPTEST, PPCAT, PPORRES, PPORRESU,
PPSTRESC, PPSTRESN, PPSTRESU, PPSPEC, PPRFTDTC

DOMAIN is "PP".
STUDYID is the supplied Protocol identifier and USUBJID is the supplied Subject
identifier.
PPSEQ is consecutive from 1 within each study and subject, ordered by ascending
reference dose date and time, analyte and standard parameter code.
For software parameter "Cmax", PPTESTCD is "CMAX" and PPTEST is "Max Conc"; for
"Tmax", they are "TMAX" and "Time of CMAX Observation"; for "AUClast", they are
"AUCLST" and "AUC to Last Nonzero Conc".
PPCAT is the analyte name.
PPORRES and PPSTRESC are the supplied Estimate text, including every reported
decimal place.
PPSTRESN is the supplied estimate as a number, without rounding.
PPORRESU and PPSTRESU are "ng/mL" for CMAX, "h" for TMAX and "h*ng/mL" for
AUCLST.
PPSPEC is "PLASMA".
PPRFTDTC is the supplied reference dose date and time.
These are the supplied noncompartmental analysis estimates, without
recalculation from concentration profiles or unit conversion.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pp.csv.
