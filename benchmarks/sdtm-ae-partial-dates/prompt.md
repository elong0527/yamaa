Following CDISC SDTM standards, use the provided ODM and DM datasets to
create an AE dataset with one record per adverse event.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, AESEQ, AETERM, AESTDTC, AEENDTC, AESTDY, AEENDY

AESEQ is the form repeat number within the subject. AESTDTC and AEENDTC
keep the collected dates at the precision collected: a full date, a
year and month, or a year alone. Nothing is imputed, and a date with no
part collected has no value. AESTDY and AEENDY count from the subject
reference start date in DM: day 1 is the reference start date, the day
before it is -1, and there is no day zero. Only a complete date gets a
study day: a partial date has none even when the reference date is
known, and no date has one when the reference date is unknown.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ae.csv.
