Following CDISC ADaM standards, use the provided ADSL, EC, EX, and FA datasets
to create an ADSL dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, DOSADJFL

DOSADJFL is "Y" when EX, EC or FA holds a qualifying record for the
subject: an EX record with EXADJ filled, an EC record with ECADJ
filled, or an FA record with FATESTCD "OCCUR", FAOBJ "DOSE
ADJUSTMENT" and FASTRESC "Y". An FA record with a test code other
than "OCCUR", such as "PRESP", or with an object other than "DOSE
ADJUSTMENT" does not qualify. DOSADJFL is "N" when the subject has
at least one record in EX, EC or FA but no qualifying record.
DOSADJFL has no value when the subject has no record in EX, EC or
FA. A source record belongs to a subject only when both its STUDYID
and its USUBJID match the subject's, and a source record for a
subject who is not in the subject list adds no record.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adsl.csv.
