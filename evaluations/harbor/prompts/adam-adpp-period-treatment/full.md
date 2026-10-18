Following CDISC ADaM standards, use the provided EX and PP datasets to create
an ADPP dataset with one record per subject, analyte, parameter and
reference date and time.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PARCAT1, PARAMCD, PARAM, PPRFTDTC, APERIOD, TRTA, DOSEA,
DOSEU, AVAL, AVALU

The parameter's dose is the subject's dose in EX that starts exactly at
the parameter's reference date and time. APERIOD is the number in that
dose's epoch, 1 for "TREATMENT 1" and 2 for "TREATMENT 2". TRTA, DOSEA
and DOSEU are that dose's treatment, dose and unit, as recorded. The full
date and time must agree: a dose earlier or later on the same day is not
the parameter's dose, and neither is the subject's most recent earlier
dose or another subject's dose at the same time. A parameter with no
such dose keeps its record, and APERIOD, TRTA, DOSEA and DOSEU have no
value on it.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpp.csv.
