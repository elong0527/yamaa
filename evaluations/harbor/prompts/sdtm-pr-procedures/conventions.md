Following CDISC SDTM standards, use the provided ODM dataset to create a
PR dataset with one record per reported procedure and per answer to the
pre-specified radiotherapy question.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PRSEQ, PRTRT, PRCAT, PRPRESP, PROCCUR, PRLOC,
PRLAT, PRDOSE, PRDOSU, PRSTDTC, PRENDTC

STUDYID is the ODM StudyOID. USUBJID is the study and subject identifiers with
a hyphen between them.

PRSEQ numbers each subject's records by PRSTDTC and then by PRTRT. PRCAT names
the form: PRIOR CANCER SURGERY or PRIOR RADIOTHERAPY. PRPRESP is Y or has no
value. PROCCUR is Y or N, or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pr.csv.
