Following CDISC ADaM standards, use the provided EX and SUBJECT_TREATMENT
datasets to create an ADEX dataset with one record per subject and treatment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXTRT, DOSECUM, NDOSREC, NDOSVAL

DOSECUM is the total of the administered doses (EXDOSE) on the EX records
for the same subject and treatment. DOSECUM has no value when no dose was
ever recorded for the treatment, while a recorded total of zero is kept
as zero. NDOSREC is the number of EX records for the subject and
treatment, and has no value when the treatment has no EX record at all.
NDOSVAL is the number of the treatment's EX records that carry a recorded
dose. An explicitly recorded zero counts as a recorded dose, and NDOSVAL
is 0 when the treatment has EX records but every dose was left blank.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
