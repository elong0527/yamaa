Following CDISC SDTM standards, use the provided NCA_TRANSFER dataset to create
a PP dataset with one record per supplied pharmacokinetic parameter estimate.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PPSEQ, PPTESTCD, PPTEST, PPCAT, PPORRES, PPORRESU,
PPSTRESC, PPSTRESN, PPSTRESU, PPSPEC, PPRFTDTC

DOMAIN is "PP"; STUDYID is the supplied Protocol identifier and USUBJID is the
supplied Subject identifier, with no added prefix or suffix.
PPSEQ is consecutive from 1 within each study and subject, in ascending
reference dose date and time, analyte and standard parameter code order.
For Parameter "Cmax", "Tmax" and "AUClast", PPTESTCD is "CMAX", "TMAX" and
"AUCLST", respectively.
PPTEST is "Max Conc" for "CMAX", "Time of CMAX Observation" for "TMAX", and
"AUC to Last Nonzero Conc" for "AUCLST".
PPCAT is the supplied Analyte name.
PPORRES and PPSTRESC are the supplied Estimate text, including all reported
decimal places and trailing zeros.
PPSTRESN is the supplied estimate as a number, without rounding.
PPORRESU and PPSTRESU are the supplied Unit: "ng/mL" for "CMAX", "h" for "TMAX"
and "h*ng/mL" for "AUCLST".
The results are the supplied noncompartmental estimates, without recalculation
or unit conversion.
PPSPEC is "PLASMA".
PPRFTDTC is the supplied ReferenceDose date and time in "YYYY-MM-DDThh:mm:ss"
form.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pp.csv.
