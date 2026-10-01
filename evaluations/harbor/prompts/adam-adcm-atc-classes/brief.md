Following CDISC ADaM standards, use the provided CM, FACM, and ATCDICT
datasets to create an ADCM dataset with one record per medication.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, CMDECOD, ATC1, ATC2, ATC3, ATC4,
ATC1CD, ATC2CD, ATC3CD, ATC4CD

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
