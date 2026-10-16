Following CDISC ADaM standards, use the provided PC and EX datasets to
create an ADPC dataset with one record per concentration record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, PCSEQ, PARAMCD, ATPT, ADTM, FANLDTM, PCRFTDTM, AFRLT, ARRLT

DRGX and MET1 belong to DRUG X, and DRGY to DRUG Y. AFRLT and ARRLT are
hours and are not rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adpc.csv.
