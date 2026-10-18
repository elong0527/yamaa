Following CDISC SDTM standards, use the provided ODM dataset to create a CM
dataset with one record per medication.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, CMSEQ, CMTRT, CMINDC, CMDOSE, CMDOSTXT, CMDOSU,
CMDOSFRQ, CMROUTE, CMSTDTC, CMENDTC, CMSTRTPT, CMSTTPT, CMENRTPT, CMENTPT

- STUDYID is the study OID and USUBJID is the subject key, each as given.
- CMSEQ is the repeat number of the medication for the subject.
- CMDOSE is the collected dose as a number when the dose text is a plain
  number, such as "100"; CMDOSE has no value otherwise.
- CMDOSTXT is the collected dose text when it is not a plain number, such
  as the range "200-400"; CMDOSTXT has no value when CMDOSE carries the
  dose.
- CMDOSFRQ is the controlled term for the collected frequency label:
  "Once daily" becomes "QD" and "Twice daily" becomes "BID".
- CMROUTE is the controlled term for the collected route label:
  "By mouth" becomes "ORAL".
- CMSTDTC is the start of the medication as collected, keeping partial
  dates at their collected precision: a start known only to year and
  month stays "2023-09".
- CMENDTC is the end of the medication as collected; CMENDTC has no value
  when no end date was collected.
- CMSTRTPT is "BEFORE" when the "taken before study" box is checked, and
  CMSTTPT is "SCREENING" then; both have no value otherwise.
- CMENRTPT is "ONGOING" when the "ongoing" box is checked, and CMENTPT is
  "END OF STUDY" then; both have no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/cm.csv.
