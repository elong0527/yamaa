Following CDISC SDTM standards, use the provided VS dataset to create
an FA dataset with one record per qualifying temperature record.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, FASEQ, FATESTCD, FATEST, FACAT, FASCAT, FAOBJ,
FAORRES, FASTRESC, VSSTRESN

Only temperature records in the reactogenicity category qualify; other
vital signs records give no row. FASEQ keeps the collected sequence
number. FATESTCD, FATEST, FACAT, FASCAT, and FAOBJ are always OCCUR,
Occurrence Indicator, REACTOGENICITY, SYSTEMIC, and FEVER. FAORRES is Y
for a temperature of 38 degrees Celsius or higher and N for a lower
one; it has no value when the result is missing or its unit is not
Celsius (C). FASTRESC repeats FAORRES. VSSTRESN keeps the collected
numeric result used for the threshold, with no value when none was
collected.

Read the source datasets from /app/input and save the completed dataset as
/app/output/fa.csv.
