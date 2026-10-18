Following CDISC SDTM standards, use the provided ODM dataset to create a PR
dataset with one record per prior-procedure form.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PRSEQ, PRTRT, PRCAT, PRPRESP, PROCCUR, PRLOC, PRLAT,
PRDOSE, PRDOSU, PRSTDTC, PRENDTC

DOMAIN is "PR". PRCAT is "PRIOR CANCER SURGERY" or "PRIOR RADIOTHERAPY".
PRPRESP is "Y" or has no value. PROCCUR is "Y" or "N", or has no value.
PRSTDTC and PRENDTC each keep the precision collected, so a date collected
as a year and month stays a year and month.

Read the source datasets from /app/input and save the completed dataset as
/app/output/pr.csv.
