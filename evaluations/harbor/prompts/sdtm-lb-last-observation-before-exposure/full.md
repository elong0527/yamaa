Following CDISC SDTM standards, use the provided ODM, DM, and LB_MAPPING
datasets to create an LB dataset with one record per collected laboratory
result.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, LBSEQ, LBTESTCD, LBSPEC, LBORRES, LBDTC, LBSTAT,
LBLOBXFL

STUDYID is the ODM StudyOID and USUBJID is the SubjectKey, each as given.

LBSEQ is the ItemGroupRepeatKey of the item group that holds the result,
so it runs across the study rather than restarting for each subject.
LBTESTCD is the test code from the item dictionary, and LBSPEC the
specimen from the same entry; a test measured in more than one specimen is
flagged separately per specimen. LBORRES is the result as collected, and
has no value on records that were not done. LBDTC is the collection date
as collected. LBSTAT is NOT DONE on records without a result.

LBLOBXFL is Y on the latest record with a result whose collection date
falls on or before the subject's reference start date, within each
subject, test, and specimen; it has no value on every other record.
Records without a result never carry the flag, even when they carry the
latest date. A subject with no reference start date carries the flag on
the latest record with a result.

Dates compare at day precision, so a record collected on the day of first
exposure still qualifies; of two results collected on the same date, the
one with the higher sequence number is flagged.

Read the source datasets from /app/input and save the completed dataset as
/app/output/lb.csv.
