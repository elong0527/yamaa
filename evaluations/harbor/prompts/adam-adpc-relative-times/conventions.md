Following CDISC ADaM standards, use the provided EX and PC datasets to create
an ADPC dataset with one record per concentration record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PCSEQ, PARAMCD, ATPT, ADTM, FANLDTM, PCRFTDTM, AFRLT, ARRLT

The analyte's drug is "DRUG X" for the parent drug DRGX and its
metabolite MET1, and "DRUG Y" for DRGY. AFRLT is not rounded. ARRLT is
not rounded. Date/times are clock readings taken as written, with no
time zone.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpc.csv.
