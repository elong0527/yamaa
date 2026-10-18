Following CDISC SDTM standards, use the provided VS_RAW dataset to create a VS
dataset with one record per collected vital-signs measurement.

The output dataset should contain the following columns in this order:
DOMAIN, STUDYID, USUBJID, VSSEQ, VISIT, VSTESTCD, VSTEST, VSORRES, VSORRESU,
VSSTRESN, VSSTRESC, VSSTRESU, VSSTAT

DOMAIN is "VS". VSSEQ is the sequence number as collected. VSTESTCD is the
test code as collected: "HEIGHT", "WEIGHT", or "TEMP". VSTEST is the test
name as collected: "Height", "Weight", or "Temperature". VSORRES is the
collected result written back as text; read as a number it drops trailing
zeros (a collected 72.50 reads 72.5), and the value itself is never
converted. VSORRESU is the unit exactly as collected: "cm", "kg", "C",
"LB", or "F", and it has no value when no result was collected. VSSTRESN
is the result as a number in the test's standard unit: a height passes
through in cm, a weight in LB is multiplied by 0.45359237 to reach kg, a
temperature in F becomes (the result minus 32) times 5 divided by 9 to
reach C, a result already in the standard unit passes through unchanged,
and a result never collected has no value. VSSTRESC is the same
standardized value written as text, so it always agrees with the numeric
result, and it has no value when no result was collected. VSSTRESU is the
test's standard unit: "cm" for height, "kg" for weight, and "C" for
temperature, and it has no value when no result was collected. VSSTAT is
"NOT DONE" when no result was collected and has no value otherwise.

Read the source datasets from /app/input and save the completed dataset as
/app/output/vs.csv.
