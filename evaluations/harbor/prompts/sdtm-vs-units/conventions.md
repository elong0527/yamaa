Following CDISC SDTM standards, use the provided VS_RAW dataset to create
a VS dataset with one record per collected vital-signs measurement.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSSTAT

VSTESTCD is HEIGHT, WEIGHT, or TEMP. VSTEST is Height, Weight, or
Temperature.

VSORRES is the collected result read as a number and written back as text,
so trailing zeros after the decimal point are dropped and a whole number has
no decimal point. VSSTRESN is not rounded. VSSTRESC is the same standardized
value written as text with at most 15 significant digits and no trailing
zeros, so a whole number has no decimal point. VSSTAT is NOT DONE or has no
value.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
