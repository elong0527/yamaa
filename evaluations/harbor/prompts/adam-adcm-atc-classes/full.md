Following CDISC ADaM standards, use the provided CM, FACM, and ATCDICT
datasets to create an ADCM dataset with one record per medication.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, CMSEQ, CMTRT, CMDECOD, ATC1, ATC2, ATC3, ATC4,
ATC1CD, ATC2CD, ATC3CD, ATC4CD

Each FACM row assigns one ATC level to one medication record, and the
dictionary maps each code to its class name. ATC1CD through ATC4CD are
the ATC codes at levels 1 to 4 for the medication's own record, and
ATC1 through ATC4 are the class names the dictionary gives for those
codes. A coded medication carries the full four-level path. A
medication with no coded name keeps all eight ATC columns empty. Path
rows that name no level from ATC1 to ATC4, or that point at no
medication record, change nothing.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adcm.csv.
