Following CDISC ADaM standards, use the provided PC and EX datasets to
create an ADPC dataset with one record per concentration record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PCSEQ, PARAMCD, ATPT, ADTM, FANLDTM, PCRFTDTM, AFRLT, ARRLT

Every PC record gives one record, including those of a subject who was
never dosed. PARAMCD is PCTESTCD and ATPT is PCTPT, as collected. ADTM is
the collection date and time from PCDTC; it has no value when the sample
was recorded with a date but no time, which is never filled in. A time
collected without seconds counts as second zero. Each analyte belongs to
a drug: DRGX and MET1 to DRUG X, and DRGY to DRUG Y. The doses of a drug
are its EX records (EXTRT), starting at EXSTDTC; a dose of another drug
is never used. FANLDTM is the start of the subject's earliest dose of the
analyte's drug, and has no value when the subject has no dose of that
drug. PCRFTDTM is the start of the reference dose: the latest dose of
that drug starting at or before ADTM, so a sample taken at a dose time
refers to that dose, and the first dose for a sample taken before every
dose. PCRFTDTM has no value when ADTM or FANLDTM has no value. AFRLT is
the hours from FANLDTM to ADTM and ARRLT the hours from PCRFTDTM to ADTM,
both negative before the first dose, not rounded, and with no value when
either date and time has no value. Date and times are clock readings with
no time zone, written as YYYY-MM-DDThh:mm:ss.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpc.csv.
