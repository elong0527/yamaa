Following CDISC ADaM standards, use the provided PP and EX datasets to
create an ADPP dataset with one record per PK parameter record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARCAT1, PARAMCD, PARAM, PPRFTDTC, APERIOD, TRTA, DOSEA,
DOSEU, AVAL, AVALU

PARCAT1 is PPCAT, PARAMCD is PPTESTCD, PARAM is PPTEST, and PPRFTDTC is
the reference date and time from PP, as collected. AVAL is PPSTRESN and
AVALU is PPSTRESU. The parameter's dose is the subject's EX dose whose
start EXSTDTC equals PPRFTDTC exactly, date and time: a dose earlier or
later on the same day, the subject's most recent earlier dose and another
subject's dose at that time are not the parameter's dose. APERIOD is the
number in that dose's EPOCH, 1 for TREATMENT 1 and 2 for TREATMENT 2.
TRTA is the dose's EXTRT, DOSEA its EXDOSE and DOSEU its EXDOSU, as
recorded. A parameter with no such dose keeps its record with APERIOD,
TRTA, DOSEA and DOSEU empty.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
