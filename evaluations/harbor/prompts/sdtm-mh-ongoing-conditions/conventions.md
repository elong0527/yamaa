Following CDISC SDTM standards, use the provided MH_FORM dataset to create an
MH dataset with one record per condition reported on the form.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, MHSEQ, MHTERM, MHSTDTC, MHENDTC, MHENRTPT, MHENTPT

MHSTDTC is the start of the condition at the precision recorded: the
year alone (for example "2015") when only the year was recorded, or the
year and month (for example "2019-03") when the month was recorded too.
MHENRTPT is "ONGOING" or "BEFORE", or has no value. MHENTPT is
"SCREENING" or has no value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/mh.csv.
