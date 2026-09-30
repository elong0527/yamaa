Following CDISC SDTM standards, use the provided ODM dataset to create a
PR dataset with one record per reported procedure and per answer to the
pre-specified radiotherapy question.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, PRSEQ, PRTRT, PRCAT, PRPRESP, PROCCUR, PRLOC,
PRLAT, PRDOSE, PRDOSU, PRSTDTC, PRENDTC

PRSEQ numbers each subject's records by PRSTDTC and then by PRTRT. PRTRT
is the reported procedure name. PRCAT names the form: PRIOR CANCER SURGERY
or PRIOR RADIOTHERAPY. PRPRESP is Y for the pre-specified radiotherapy
question and has no value for the reported surgery. PROCCUR is Y or N for
the pre-specified question and has no value for the reported surgery.
PRLOC and PRLAT carry the surgery site, and have no value for
radiotherapy. PRDOSE and PRDOSU carry the radiotherapy dose, and have no
value when the procedure did not occur and for surgery. PRSTDTC and
PRENDTC keep the collected precision: a year-month surgery date stays
year-month, while the radiotherapy course carries full start and end
dates.

A "No" answer is still a record, with no dose and no dates: it tells "no
prior radiotherapy" apart from "never asked".

Read the source datasets from /app/input and save the completed dataset as
/app/output/pr.csv.
