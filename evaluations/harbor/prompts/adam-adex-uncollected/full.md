Following CDISC ADaM standards, use the provided ADSL and EX datasets to
create an ADEX dataset with one record per subject.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, TRT01P, DOSECUM, NDOSREC, NDOSVAL

All recorded EXDOSE values are in milligrams (EXDOSU=mg), including doses
of different treatments; any other units have been standardized before
these inputs are supplied. DOSECUM is therefore expressed in milligrams.
Reject a recorded dose with a different unit or no unit rather than adding
it to the total.

TRT01P is the subject's planned treatment from ADSL, and every ADSL subject
has a record. DOSECUM is the total of the administered doses (EXDOSE) on the
subject's EX records. DOSECUM has no value when no dose was ever recorded
for the subject, while a recorded total of zero is kept as zero. NDOSREC is
the number of the subject's EX records, and has no value when the subject
has no EX record at all. NDOSVAL is the number of the subject's EX records
that carry a recorded dose. An explicitly recorded zero counts as a recorded
dose, and NDOSVAL is 0 when the subject has EX records but every dose was
left blank.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
