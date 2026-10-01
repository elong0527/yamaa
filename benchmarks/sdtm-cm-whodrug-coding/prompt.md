Following CDISC SDTM standards, use the provided ODM, CODING, and
WHODRUG datasets to create a CM dataset with one record per reported
medication.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMDECOD, CMCLAS, CMCLASCD

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

CMSEQ is the medication repeat number within the subject. CMTRT keeps
the medication name exactly as written. CMDECOD is the preferred name
of the drug record the coder chose for the reported name, and has no
value when the reported name is not yet coded. CMCLAS is the class name
of the ATC code the coder assigned for this use, and CMCLASCD is the
code of that class; both have no value when the reported name is not
yet coded. One drug can carry several ATC codes, and the coder assigns
the class for the use at hand, so the same drug record can give a
different class on different records. A record with no coding, or a
code not in the extract, leaves the coded variables with no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/cm.csv.
