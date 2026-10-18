Following CDISC SDTM standards, use the provided CODING_OUTPUT, ODM, and
WHODRUG_EXTRACT datasets to create a CM dataset with one record per
reported medication.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMDECOD, CMCLAS, CMCLASCD

STUDYID is the study OID in ODM as given, and USUBJID is the subject key
in ODM as given. CMSEQ is the repeat number of the reported medication
for the subject. CMTRT is the medication name as reported, the value of
the IT.CM.CMTRT item, kept exactly as written. Each record has the drug
record and ATC code the coder's output assigns to its STUDYID, USUBJID,
and CMSEQ. CMDECOD is the preferred name in the WHODrug extract for the
drug record the coder chose. CMCLAS is the ATC class name in the WHODrug
extract for that drug record together with the ATC code the coder
assigned to the record. CMCLASCD is the ATC class code in the WHODrug
extract for that same drug record and ATC code. CMDECOD, CMCLAS, and
CMCLASCD have no value when the reported name is not yet coded. The same
drug record can code to a different CMCLAS on different records, because
the class is the one for the ATC code the coder assigned for the use at
hand.

Read the source datasets from /app/input and save the completed dataset as
/app/output/cm.csv.
