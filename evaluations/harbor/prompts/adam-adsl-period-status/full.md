Following CDISC ADaM standards, use the provided DM, DS, and EX datasets to
create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EOP01STT, DCP01RS, EOP02STT, DCP02RS

Period 1 is the "TREATMENT 1" epoch and period 2 is the "TREATMENT 2"
epoch. A period's status reads the subject's disposition event for that
period, the DS record with category "DISPOSITION EVENT" in that period's
epoch. The epoch alone places an event in a period; its date, complete
or partial, plays no part. Records in other categories, such as a site
transfer, and events in other epochs, such as a withdrawal during the
washout or the end-of-study event, never set a period's status.
EOP01STT is "COMPLETED" when the coded term of the period 1 event is
"COMPLETED", "DISCONTINUED" when its coded term is any other value,
"ONGOING" when the subject received a dose in period 1 but has no such
event, and has no value when the subject never received a dose in
period 1. DCP01RS is the coded term of the period 1 event, never its
reported term, when EOP01STT is "DISCONTINUED", and has no value
otherwise. EOP02STT is "COMPLETED" when the coded term of the period 2
event is "COMPLETED", "DISCONTINUED" when its coded term is any other
value, "ONGOING" when the subject received a dose in period 2 but has
no such event, and has no value when the subject never received a dose
in period 2. DCP02RS is the coded term of the period 2 event, never its
reported term, when EOP02STT is "DISCONTINUED", and has no value
otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
