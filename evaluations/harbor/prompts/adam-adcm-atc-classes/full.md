Following CDISC ADaM standards, use the provided ATC_DICT, CM, and FACM
datasets to create an ADCM dataset with one record per medication record.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, CMDECOD, ATC1, ATC2, ATC3, ATC4, ATC1CD,
ATC2CD, ATC3CD, ATC4CD

ATC1CD, ATC2CD, ATC3CD, and ATC4CD hold the codes in FAORRES on the
medication's FACM rows whose FATESTCD is ATC1, ATC2, ATC3, and ATC4,
matched to the medication record by STUDYID, USUBJID, and CMSEQ. ATC1,
ATC2, ATC3, and ATC4 hold the class name in ATCNAME in the ATC_DICT row
whose ATCCODE is the medication's ATC1CD, ATC2CD, ATC3CD, and ATC4CD. A
medication with no coded name and no FACM rows for it has no value in
ATC1, ATC2, ATC3, ATC4, ATC1CD, ATC2CD, ATC3CD, and ATC4CD. FACM rows
whose FATESTCD is none of ATC1 to ATC4, and FACM rows that match no
medication record, change nothing.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
