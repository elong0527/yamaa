Following CDISC SDTM standards, use the provided MH_FORM dataset to create
an MH dataset with one record per reported condition.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, MHSEQ, MHTERM, MHSTDTC, MHENDTC, MHENRTPT, MHENTPT

MHTERM is the condition as written on the form. MHSTDTC is the start of
the condition at the precision recorded: the year alone when only the year
was known, or year and month; it has no value when no year was recorded.
MHENDTC is the date the condition ended, when an end was recorded; a
condition still active at the screening visit carries no end date.

MHENRTPT says how the condition's end relates to the screening visit:
ONGOING when the tick box said it was still active, BEFORE when the
recorded end date comes before the screening visit date recorded on the
form, and no value otherwise (for example when the recorded end date is on
or after the screening visit date). MHENTPT names the visit the end
reference is measured against, SCREENING; it has no value when there is no
end reference.

Read the source datasets from /app/input and save the completed dataset as
/app/output/mh.csv.
