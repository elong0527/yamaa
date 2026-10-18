Following CDISC SDTM standards, use the provided EX_RAW dataset to create an EX
dataset with one record per administered combination component.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXSTDTC, EXENDTC, EXADJ

DOMAIN is "EX". EXSEQ numbers each subject's administrations from 1, by
start date, and on the same start date by treatment name. EXDOSU is the
dose unit as collected, and "mg", "mg/m2", and "AUC" remain distinct.
EXADJ is the adjustment reason as collected, for example "DOSE
INTERRUPTED", and has no value when no reason was reported. An
administered component with a dose of 0, such as a saline placebo, keeps
0 as its EXDOSE: a zero dose is not an uncollected dose.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
