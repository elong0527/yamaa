Following CDISC ADaM standards, use the provided EX and SUBJECT_TREATMENT
datasets to create an ADEX dataset with one record per subject per
treatment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXTRT, EXDOSU, DOSECUM, NCYCLES, RDI

DOSECUM is the total administered dose across the subject's exposure
records for the treatment. A record with no collected dose adds nothing
to the total. A duplicated exposure record counts once per entry, so its
dose enters the total twice. DOSECUM has no value when the treatment has
no exposure records. A treatment whose exposure records all hold a zero
dose has a DOSECUM of 0. NCYCLES is the number of the subject's exposure
records for the treatment. Every exposure record counts toward NCYCLES,
even one with a zero or missing dose. NCYCLES has no value when the
treatment has no exposure records. RDI is the cumulative dose as a
percentage of the planned total dose, which is the planned dose per cycle
multiplied by the planned number of cycles. RDI has no value when the
planned total dose is zero or when DOSECUM has no value. RDI is not
rounded.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
