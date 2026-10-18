Following CDISC ADaM standards, use the provided EX and PC datasets to create
an ADPC dataset with one record per concentration record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PCSEQ, PARAMCD, ATPT, ADTM, FANLDTM, PCRFTDTM, AFRLT, ARRLT

Each concentration record gives one output record, identified by its
sequence number PCSEQ, and the records of a subject who was never dosed
stay. PARAMCD is the analyte as collected in PCTESTCD, and ATPT is the
planned time point as collected in PCTPT. ADTM is the collection date
and time in PCDTC. ADTM has no value when the sample was recorded with
a date but no time, and that time is never filled in. The analyte's
drug is "DRUG X" for the parent drug DRGX and its metabolite MET1, and
"DRUG Y" for DRGY. FANLDTM is the start date and time of the subject's
earliest dose of the analyte's drug. FANLDTM has no value when the
subject has no dose of that drug. A dose of another drug is never a
first dose or a reference dose. PCRFTDTM is the start date and time of
the reference dose, the latest dose of the analyte's drug starting at
or before the sample, so a sample taken at a dose time refers to that
dose. For a sample taken before every dose, the reference dose is the
first dose. PCRFTDTM has no value when ADTM or FANLDTM has no value.
AFRLT is the number of hours from FANLDTM to ADTM, not rounded. AFRLT
is negative for a sample taken before the first dose, and it has no
value when FANLDTM or ADTM has no value. ARRLT is the number of hours
from PCRFTDTM to ADTM, not rounded. ARRLT starts again at each dose
while AFRLT keeps growing, and ARRLT is negative only for a sample
taken before the first dose. ARRLT has no value when PCRFTDTM or ADTM
has no value. Date/times are clock readings taken as written, with no
time zone.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpc.csv.
