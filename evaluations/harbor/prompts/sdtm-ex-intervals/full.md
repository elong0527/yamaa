Following CDISC SDTM standards, use the provided EC_RAW dataset to create an EX
dataset with one record per constant-dose interval.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXDOSFRQ, EXSTDTC,
EXENDTC, EXADJ

DOMAIN is "EX". The subject's administrations of one treatment at one
dose, dose unit, and dosing frequency form one interval, and each
interval gives one record. EXSTDTC is the first administration date at
the interval's dose level. EXENDTC is the last administration date at
the interval's dose level. EXADJ is the adjustment reason recorded on
any administration in the interval, for example "DOSE REDUCED" or
"DOSE INTERRUPTED", and has no value when no administration in the
interval records a reason. A dose interruption is its own interval,
with a zero dose and the reason recorded. EXSEQ numbers the subject's
intervals from 1 in the order they started, by start date then
treatment.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
