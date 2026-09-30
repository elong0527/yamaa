Following CDISC SDTM standards, use the provided EX_RAW dataset to
create an EX dataset with one record per administered combination
component.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, EXSEQ, EXTRT, EXDOSE, EXDOSU, EXSTDTC, EXENDTC,
EXADJ

EXSEQ numbers the administrations within the subject by start date and
then, on the same date, by treatment name. EXDOSU is the dose unit as
collected: milligrams (mg), milligrams per square metre (mg/m2), and area
under the curve (AUC) remain distinct. EXADJ is the adjustment reason as
collected, with no value when none was reported. A saline placebo
component is administered, so it keeps its EXDOSE of 0; a zero dose is not
an uncollected dose.

Read the source datasets from /app/input and save the completed dataset as
/app/output/ex.csv.
