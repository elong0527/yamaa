Following CDISC ADaM standards, use the provided TRT and EX datasets to
create an ADEX dataset with one record per subject per treatment.

The output dataset should contain the following columns in this order:
STUDYID, USUBJID, EXTRT, EXDOSU, DOSECUM, NCYCLES, RDI

DOSECUM is the total administered dose across the subject's exposure
records for the treatment. A record with no collected dose adds
nothing; a planned treatment with no exposure records has an empty
total. NCYCLES is the number of exposure records for the treatment.
Every record counts, even one with a zero or missing dose. A planned
treatment with no exposure records has an empty count. RDI is
cumulative dose as a percentage of the planned total dose, the planned
dose per cycle times the planned number of cycles, and is not rounded.
It has no value when the planned total is zero or when there is no
cumulative dose to compare. An administered zero dose adds nothing to
the total but its record still counts. A duplicated exposure record
counts once per entry, so its dose enters the total twice.

Read the source datasets from /app/input and save the completed dataset as
/app/output/adex.csv.
