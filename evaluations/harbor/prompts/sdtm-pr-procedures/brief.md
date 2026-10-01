Following CDISC SDTM standards, use the provided ODM dataset to create a
PR dataset with one record per reported procedure and per answer to the
pre-specified radiotherapy question.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PRSEQ, PRTRT, PRCAT, PRPRESP, PROCCUR, PRLOC,
PRLAT, PRDOSE, PRDOSU, PRSTDTC, PRENDTC

Read the source datasets from /app/input and save the completed dataset as
/app/output/pr.csv.
