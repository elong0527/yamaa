Following CDISC SDTM standards, use the provided ODM dataset to create a DM
dataset with one record per subject.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, SUBJID, BRTHDTC, AGE, AGEU, SEX

The ODM dataset is an ODM 1.3 XML snapshot export: the current value of
every collected item, with audit records, signatures and comments kept
beside the data, which add no values. STUDYID is the study OID and SUBJID
is the subject key, each as given, and USUBJID is the study OID and the
subject key joined by a hyphen. BRTHDTC is the IT.DM.BRTHDAT item's value
as collected, a year and month when the day is unknown, and has no value
when the item is recorded as null. AGE is the IT.DM.AGE item's value, and
AGEU is "YEARS" whenever AGE has a value. SEX is the IT.DM.SEX item's
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/dm.csv.
