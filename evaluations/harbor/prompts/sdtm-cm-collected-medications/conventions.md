Following CDISC SDTM standards, use the provided ODM dataset to create a CM
dataset with one record per medication.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMINDC, CMDOSE, CMDOSTXT, CMDOSU,
CMDOSFRQ, CMROUTE, CMSTDTC, CMENDTC, CMSTRTPT, CMSTTPT, CMENRTPT, CMENTPT

- CMSTDTC keeps a partial date at its collected precision: a start known
  only to year and month stays "2023-09".
- CMSTRTPT is "BEFORE" or has no value, and CMSTTPT is "SCREENING" or has
  no value.
- CMENRTPT is "ONGOING" or has no value, and CMENTPT is "END OF STUDY" or
  has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/cm.csv.
