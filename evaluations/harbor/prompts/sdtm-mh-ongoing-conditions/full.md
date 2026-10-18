Following CDISC SDTM standards, use the provided MH_FORM dataset to create an
MH dataset with one record per condition reported on the form.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, MHSEQ, MHTERM, MHSTDTC, MHENDTC, MHENRTPT, MHENTPT

MHSEQ is the sequence number of the condition as numbered on the form.
MHSTDTC is the start of the condition at the precision recorded: the
year alone (for example "2015") when only the year was recorded, or the
year and month (for example "2019-03") when the month was recorded too.
MHSTDTC has no value when no start year was recorded. A condition still
active at the screening visit carries no end date: the ongoing answer
is the record of its end status. MHENRTPT is "ONGOING" when the ongoing
tick box on the form said the condition was still active. MHENRTPT is
"BEFORE" when the recorded end date comes before the screening visit
date recorded on the form. MHENRTPT has no value otherwise, for example
when the recorded end date is on or after the screening visit date.
MHENTPT is "SCREENING" when MHENRTPT has a value, and has no value when
there is no end reference.

Read the source datasets from /app/input and save the completed dataset as
/app/output/mh.csv.
